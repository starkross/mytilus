//! `mytilus-prng` — rand/random and the *rand48 family.
//!
//! Mirrors `src/prng/` upstream. All deterministic, no syscalls. Note that
//! `rand` and `srand` share global state and are NOT thread-safe (matching
//! upstream and the C standard); `random` and friends ARE thread-safe via an
//! internal spinlock.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use core::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

use mytilus_sys::ctypes::{c_char, c_int, c_long, c_uint, c_ushort, size_t};

// ---------------------------------------------------------------------------
// rand / srand  (rand.c)
// ---------------------------------------------------------------------------
//
// 64-bit LCG; not thread-safe. Same constants as upstream.

static mut RAND_SEED: u64 = 0;

#[no_mangle]
pub extern "C" fn srand(s: c_uint) {
    // SAFETY: rand/srand are documented non-thread-safe; single-threaded
    // access to RAND_SEED is the contract.
    unsafe {
        RAND_SEED = (s as u64).wrapping_sub(1);
    }
}

#[no_mangle]
pub extern "C" fn rand() -> c_int {
    // SAFETY: see srand.
    unsafe {
        RAND_SEED = 6_364_136_223_846_793_005u64
            .wrapping_mul(RAND_SEED)
            .wrapping_add(1);
        (RAND_SEED >> 33) as c_int
    }
}

// ---------------------------------------------------------------------------
// rand_r  (rand_r.c)
// ---------------------------------------------------------------------------
//
// Re-entrant: caller owns the seed slot. Uses a 32-bit LCG with the MT19937
// tempering as a final mix, divided by 2 so it fits in `int`.

fn temper(mut x: u32) -> u32 {
    x ^= x >> 11;
    x ^= (x << 7) & 0x9D2C_5680;
    x ^= (x << 15) & 0xEFC6_0000;
    x ^= x >> 18;
    x
}

/// # Safety
/// `seed` must point to a valid, writable `c_uint`.
#[no_mangle]
pub unsafe extern "C" fn rand_r(seed: *mut c_uint) -> c_int {
    // SAFETY: caller-provided pointer to a c_uint, per contract.
    let s = unsafe { &mut *seed };
    *s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    (temper(*s) / 2) as c_int
}

// ---------------------------------------------------------------------------
// random / srandom / initstate / setstate  (random.c)
// ---------------------------------------------------------------------------
//
// Lagged-Fibonacci LFSR, falling back to a 31-bit LCG when the state buffer
// is too small. Thread-safe via an internal spinlock.
//
// TODO(mytilus-thread): replace the CAS spinlock below with `__lock`/`__unlock`
//   from mytilus-thread. The spinlock is a placeholder — musl's real lock
//   blocks on a futex via `__wait`; we can't call that yet because the futex
//   syscall wrapper, `__pthread_self`, and the cancellation machinery aren't
//   in place.
// TODO(mytilus-process): expose `__random_lockptr` (`volatile int *const`
//   pointing at the lock) for fork's lock-reset path. Deferred because the
//   only consumer is `src/process/fork.c` and we have no fork wrapper yet.

const N_DEFAULT: c_int = 31;

#[allow(clippy::unreadable_literal)]
static mut RANDOM_INIT: [u32; 32] = [
    0x00000000, 0x5851f42d, 0xc0b18ccf, 0xcbb5f646, 0xc7033129, 0x30705b04, 0x20fd5db4, 0x9a8b7f78,
    0x502959d8, 0xab894868, 0x6c0356a7, 0x88cdb7ff, 0xb477d43f, 0x70a3a52b, 0xa8e4baf1, 0xfd8341fc,
    0x8ae16fd9, 0x742d2f7a, 0x0d1f0796, 0x76035e09, 0x40f7702c, 0x6fa72ca5, 0xaaa84157, 0x58a0df74,
    0xc74a0364, 0xae533cc4, 0x04185faf, 0x6de3b115, 0x0cab8628, 0xf043bfa4, 0x398150e9, 0x37521657,
];

static mut RAND_N: c_int = N_DEFAULT;
static mut RAND_I: c_int = 3;
static mut RAND_J: c_int = 0;
// Initialized lazily under RANDOM_LOCK to point at &RANDOM_INIT[1]; can't be
// done in a const initializer because the address of a `static mut` is not
// const-evaluable.
static RAND_X: AtomicPtr<u32> = AtomicPtr::new(core::ptr::null_mut());

static RANDOM_LOCK: AtomicI32 = AtomicI32::new(0);

fn lock() {
    while RANDOM_LOCK
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

fn unlock() {
    RANDOM_LOCK.store(0, Ordering::Release);
}

/// Returns the current `x` pointer, lazily initializing it on first call.
/// Must be called with `RANDOM_LOCK` held.
fn x_ptr() -> *mut u32 {
    let p = RAND_X.load(Ordering::Relaxed);
    if !p.is_null() {
        return p;
    }
    let init = core::ptr::addr_of_mut!(RANDOM_INIT) as *mut u32;
    // SAFETY: pointing one element past the start of a 32-element static.
    let p = unsafe { init.add(1) };
    RAND_X.store(p, Ordering::Relaxed);
    p
}

fn lcg31(x: u32) -> u32 {
    1_103_515_245u32.wrapping_mul(x).wrapping_add(12_345) & 0x7fff_ffff
}

fn lcg64(x: u64) -> u64 {
    6_364_136_223_846_793_005u64.wrapping_mul(x).wrapping_add(1)
}

/// Pack `n,i,j` into the 32-bit word at `x[-1]` and return `x-1`.
/// Caller must hold the lock.
unsafe fn savestate() -> *mut u32 {
    let x = x_ptr();
    // SAFETY: x always points one element into a 32-element-or-larger buffer.
    unsafe {
        let n = RAND_N as u32;
        let i = RAND_I as u32;
        let j = RAND_J as u32;
        *x.sub(1) = (n << 16) | (i << 8) | j;
        x.sub(1)
    }
}

/// Adopt `state` as the new buffer and unpack `n,i,j` from `state[0]`.
/// Caller must hold the lock.
unsafe fn loadstate(state: *mut u32) {
    // SAFETY: caller asserts state points at a valid LFSR state buffer with
    // at least 4 bytes for the header word.
    unsafe {
        let x = state.add(1);
        RAND_X.store(x, Ordering::Relaxed);
        let header = *state;
        RAND_N = (header >> 16) as c_int;
        RAND_I = ((header >> 8) & 0xff) as c_int;
        RAND_J = (header & 0xff) as c_int;
    }
}

/// Re-seed the active state buffer. Caller must hold the lock.
unsafe fn srandom_locked(seed: c_uint) {
    let x = x_ptr();
    // SAFETY: x is the active state buffer of size at least RAND_N words.
    unsafe {
        if RAND_N == 0 {
            *x = seed;
            return;
        }
        RAND_I = if RAND_N == 31 || RAND_N == 7 { 3 } else { 1 };
        RAND_J = 0;
        let mut s = seed as u64;
        for k in 0..RAND_N {
            s = lcg64(s);
            *x.add(k as usize) = (s >> 32) as u32;
        }
        // Make sure x contains at least one odd number.
        *x |= 1;
    }
}

#[no_mangle]
pub extern "C" fn srandom(seed: c_uint) {
    lock();
    // SAFETY: lock held.
    unsafe {
        srandom_locked(seed);
    }
    unlock();
}

#[no_mangle]
pub extern "C" fn random() -> c_long {
    lock();
    // SAFETY: lock held; x_ptr/savestate/loadstate maintain the invariants.
    let k = unsafe {
        let x = x_ptr();
        if RAND_N == 0 {
            let v = lcg31(*x);
            *x = v;
            v as c_long
        } else {
            let i = RAND_I as usize;
            let j = RAND_J as usize;
            let n = RAND_N as usize;
            let v = (*x.add(i)).wrapping_add(*x.add(j));
            *x.add(i) = v;
            RAND_I = if i + 1 == n { 0 } else { (i + 1) as c_int };
            RAND_J = if j + 1 == n { 0 } else { (j + 1) as c_int };
            (v >> 1) as c_long
        }
    };
    unlock();
    k
}

/// # Safety
/// `state` must point to at least `size` writable bytes, aligned for `u32`.
#[no_mangle]
pub unsafe extern "C" fn initstate(seed: c_uint, state: *mut c_char, size: size_t) -> *mut c_char {
    if size < 8 {
        return core::ptr::null_mut();
    }
    lock();
    // SAFETY: lock held; state is asserted valid by the caller.
    let old = unsafe {
        let old = savestate();
        RAND_N = if size < 32 {
            0
        } else if size < 64 {
            7
        } else if size < 128 {
            15
        } else if size < 256 {
            31
        } else {
            63
        };
        RAND_X.store((state as *mut u32).add(1), Ordering::Relaxed);
        srandom_locked(seed);
        savestate();
        old
    };
    unlock();
    old as *mut c_char
}

/// # Safety
/// `state` must point to a buffer previously produced by `initstate` or
/// `setstate` (or be a valid LFSR state of matching shape).
#[no_mangle]
pub unsafe extern "C" fn setstate(state: *mut c_char) -> *mut c_char {
    lock();
    // SAFETY: lock held; state is asserted valid by the caller.
    let old = unsafe {
        let old = savestate();
        loadstate(state as *mut u32);
        old
    };
    unlock();
    old as *mut c_char
}

// ---------------------------------------------------------------------------
// 48-bit family  (__rand48_step.c, __seed48.c, *rand48.c, seed48.c, srand48.c,
// lcong48.c)
// ---------------------------------------------------------------------------
//
// __seed48 is a 7-element array: indices 0..2 hold the 48-bit state
// (little-endian halves), 3..5 hold the LCG multiplier, 6 holds the addend.
// Default constants give the standard a=0x5deece66d, c=0xb.

#[no_mangle]
#[allow(clippy::unreadable_literal)]
pub static mut __seed48: [c_ushort; 7] = [0, 0, 0, 0xe66d, 0xdeec, 0x5, 0xb];

/// # Safety
/// `xi` must point to 3 writable `c_ushort`s; `lc` must point to 4 readable
/// `c_ushort`s (multiplier 0..2 + addend at 3).
#[no_mangle]
pub unsafe extern "C" fn __rand48_step(xi: *mut c_ushort, lc: *const c_ushort) -> u64 {
    // SAFETY: caller asserts both pointers are valid for the named element
    // counts.
    unsafe {
        let xi0 = *xi as u64;
        let xi1 = *xi.add(1) as u64;
        let xi2 = *xi.add(2) as u64;
        let x = xi0 | (xi1 << 16) | (xi2 << 32);

        let lc0 = *lc as u64;
        let lc1 = *lc.add(1) as u64;
        let lc2 = *lc.add(2) as u64;
        let lc3 = *lc.add(3) as u64;
        let a = lc0 | (lc1 << 16) | (lc2 << 32);

        let x = a.wrapping_mul(x).wrapping_add(lc3);
        *xi = x as c_ushort;
        *xi.add(1) = (x >> 16) as c_ushort;
        *xi.add(2) = (x >> 32) as c_ushort;
        x & 0xffff_ffff_ffff
    }
}

fn seed48_lc_ptr() -> *const c_ushort {
    // SAFETY: taking the address of a static; addr_of doesn't dereference.
    unsafe { core::ptr::addr_of!(__seed48[3]) }
}

fn seed48_xi_ptr() -> *mut c_ushort {
    // SAFETY: taking the address of a static; addr_of_mut doesn't dereference.
    unsafe { core::ptr::addr_of_mut!(__seed48[0]) }
}

/// # Safety
/// `s` must point to 3 writable `c_ushort`s.
#[no_mangle]
pub unsafe extern "C" fn erand48(s: *mut c_ushort) -> f64 {
    // SAFETY: forwarded from caller.
    let step = unsafe { __rand48_step(s, seed48_lc_ptr()) };
    let bits = 0x3ff0_0000_0000_0000_u64 | (step << 4);
    f64::from_bits(bits) - 1.0
}

#[no_mangle]
pub extern "C" fn drand48() -> f64 {
    // SAFETY: __seed48 is statically valid.
    unsafe { erand48(seed48_xi_ptr()) }
}

/// # Safety
/// `s` must point to 3 writable `c_ushort`s.
#[no_mangle]
pub unsafe extern "C" fn nrand48(s: *mut c_ushort) -> c_long {
    // SAFETY: forwarded from caller.
    let step = unsafe { __rand48_step(s, seed48_lc_ptr()) };
    (step >> 17) as c_long
}

#[no_mangle]
pub extern "C" fn lrand48() -> c_long {
    // SAFETY: __seed48 is statically valid.
    unsafe { nrand48(seed48_xi_ptr()) }
}

/// # Safety
/// `s` must point to 3 writable `c_ushort`s.
#[no_mangle]
pub unsafe extern "C" fn jrand48(s: *mut c_ushort) -> c_long {
    // SAFETY: forwarded from caller.
    let step = unsafe { __rand48_step(s, seed48_lc_ptr()) };
    // Top 32 bits, then sign-extended to long via the i32 cast (matches
    // upstream's `(int32_t)(step>>16)`).
    ((step >> 16) as i32) as c_long
}

#[no_mangle]
pub extern "C" fn mrand48() -> c_long {
    // SAFETY: __seed48 is statically valid.
    unsafe { jrand48(seed48_xi_ptr()) }
}

/// # Safety
/// `s` must point to 3 readable `c_ushort`s.
#[no_mangle]
pub unsafe extern "C" fn seed48(s: *mut c_ushort) -> *mut c_ushort {
    // Hold the previous state across the swap so we can return it.
    static mut PREV: [c_ushort; 3] = [0; 3];
    // SAFETY: __seed48 and PREV are statically valid; s is asserted by caller.
    unsafe {
        PREV[0] = __seed48[0];
        PREV[1] = __seed48[1];
        PREV[2] = __seed48[2];
        __seed48[0] = *s;
        __seed48[1] = *s.add(1);
        __seed48[2] = *s.add(2);
        core::ptr::addr_of_mut!(PREV) as *mut c_ushort
    }
}

#[no_mangle]
pub extern "C" fn srand48(seed: c_long) {
    let mut s: [c_ushort; 3] = [0x330e, seed as c_ushort, (seed >> 16) as c_ushort];
    // SAFETY: s is a stack array of length 3.
    unsafe {
        seed48(s.as_mut_ptr());
    }
}

/// # Safety
/// `p` must point to 7 readable `c_ushort`s.
#[no_mangle]
pub unsafe extern "C" fn lcong48(p: *mut c_ushort) {
    // SAFETY: forwarded from caller; __seed48 is statically valid. Use raw
    // pointers so we don't take a `&mut` to a `static mut` (Rust 2024 lint).
    unsafe {
        let dst = core::ptr::addr_of_mut!(__seed48) as *mut c_ushort;
        for i in 0..7 {
            *dst.add(i) = *p.add(i);
        }
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // Most of these touch shared globals (RAND_SEED, __seed48, the LFSR
    // state). cargo runs tests in parallel — serialize anything that mutates
    // a shared seed.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn rand_after_srand_one() {
        let _g = TEST_LOCK.lock().unwrap();
        // srand(1) -> seed = 0; first rand: seed = 1, return 1>>33 = 0.
        srand(1);
        assert_eq!(rand(), 0);
    }

    #[test]
    fn rand_is_deterministic() {
        let _g = TEST_LOCK.lock().unwrap();
        srand(42);
        let a = (rand(), rand(), rand());
        srand(42);
        let b = (rand(), rand(), rand());
        assert_eq!(a, b);
        // Reference values from musl's `seed = 6364136223846793005*seed + 1;
        // return seed >> 33` applied to seed=41 (i.e. srand(42)). These are
        // part of the public ABI of musl's rand().
        assert_eq!(a, (311_430_560, 131_117_839, 1_110_653_038));
    }

    #[test]
    fn rand_r_is_pure_function_of_seed() {
        // No shared state — no lock needed.
        let mut s1: c_uint = 0xdead_beef;
        let mut s2: c_uint = 0xdead_beef;
        // SAFETY: locals; pointers valid.
        unsafe {
            assert_eq!(rand_r(&mut s1), rand_r(&mut s2));
            assert_eq!(rand_r(&mut s1), rand_r(&mut s2));
            assert_eq!(s1, s2);
        }
    }

    #[test]
    fn drand48_in_unit_interval() {
        let _g = TEST_LOCK.lock().unwrap();
        srand48(123);
        for _ in 0..100 {
            let v = drand48();
            assert!((0.0..1.0).contains(&v), "drand48 out of [0,1): {v}");
        }
    }

    #[test]
    fn lrand48_nonneg_and_31bit() {
        let _g = TEST_LOCK.lock().unwrap();
        srand48(7);
        for _ in 0..100 {
            let v = lrand48();
            assert!(v >= 0);
            assert!(v < (1i64 << 31));
        }
    }

    #[test]
    fn mrand48_within_i32_range() {
        let _g = TEST_LOCK.lock().unwrap();
        srand48(7);
        for _ in 0..100 {
            let v = mrand48();
            assert!((i32::MIN as c_long..=i32::MAX as c_long).contains(&v));
        }
    }

    /// POSIX-stable: first lrand48/mrand48/drand48 after srand48(0)/srand48(123)
    /// are fully determined by the spec.
    #[test]
    fn rand48_spec_first_outputs() {
        let _g = TEST_LOCK.lock().unwrap();
        srand48(0);
        assert_eq!(lrand48(), 366_850_414);
        srand48(0);
        assert_eq!(mrand48(), 733_700_828);
        srand48(123);
        // Bit-exact f64 per the *rand48 spec; cross-checked against an
        // independent Python implementation of the LCG.
        assert_eq!(drand48().to_bits(), 0x3fd1_e386_4ed4_4040);
    }

    #[test]
    fn srand48_then_seed48_round_trip() {
        let _g = TEST_LOCK.lock().unwrap();
        srand48(99);
        // SAFETY: passing a 3-element local.
        let prev = unsafe {
            let mut new_state: [c_ushort; 3] = [1, 2, 3];
            seed48(new_state.as_mut_ptr())
        };
        // SAFETY: prev points at a 3-element static returned by seed48.
        unsafe {
            assert_eq!(*prev, 0x330e);
            assert_eq!(*prev.add(1), 99);
            assert_eq!(*prev.add(2), 0);
        }
    }

    #[test]
    fn random_is_deterministic() {
        let _g = TEST_LOCK.lock().unwrap();
        srandom(2024);
        let a: [c_long; 5] = [random(), random(), random(), random(), random()];
        srandom(2024);
        let b: [c_long; 5] = [random(), random(), random(), random(), random()];
        assert_eq!(a, b);
    }

    #[test]
    fn random_default_state_31bit() {
        let _g = TEST_LOCK.lock().unwrap();
        srandom(1);
        for _ in 0..200 {
            let v = random();
            assert!(v >= 0);
            assert!(v < (1i64 << 31));
        }
    }

    #[test]
    fn rand48_step_matches_known_first_call() {
        let _g = TEST_LOCK.lock().unwrap();
        // After srand48(0): xi = [0x330e, 0, 0]; lc constants are the default.
        // Step computes: a = 0x5deece66d, x = 0x330e, x' = a*x + 0xb mod 2^48.
        // a*x = 0x5deece66d * 0x330e = 0x132f02ee37b302
        // Mask to 48 bits: 0x32f02ee37b302  (top hex digit clipped)
        // + 0xb           = 0x32f02ee37b30d
        // Wait — 0x5deece66d * 0x330e fits in 64 bits (a is 36-bit, x is 14-bit
        // → product fits in 50 bits). Mask after addition.
        let a: u64 = 0x5deece66d;
        let x0: u64 = 0x330e;
        let expected = a.wrapping_mul(x0).wrapping_add(0xb) & 0xffff_ffff_ffff;
        srand48(0);
        // SAFETY: __seed48 statically valid.
        let got = unsafe { __rand48_step(seed48_xi_ptr(), seed48_lc_ptr()) };
        assert_eq!(got, expected);
    }
}
