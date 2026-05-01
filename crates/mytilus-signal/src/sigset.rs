//! `sigset_t` and its eight bit-manipulators.
//!
//! Mirrors `src/signal/sig{empty,fill,add,del,is}set.c` and
//! `src/signal/{sigorset,sigandset,sigisemptyset}.c` upstream.
//!
//! ABI shape: `sigset_t` is a struct holding `unsigned long __bits[128/sizeof(long)]`.
//! On LP64 that's 16 × `u64` = 128 bytes. The 128-byte size is dictated by
//! the kernel's view of sigset_t (large enough for any future signal count);
//! actual signal bits only occupy `_NSIG/8 = 8` bytes (one `u64`) on Linux,
//! so most of the struct is "don't-care" padding for ABI compat.
//!
//! Reserved signals 32, 33, 34: musl uses these internally for thread
//! cancellation (`SIGCANCEL`), the "synccall" broadcast, and `setxid`. Adding
//! or removing them via `sigaddset`/`sigdelset` returns `EINVAL`. `sigismember`
//! does NOT reject them — applications can still query whether they're set,
//! they just can't be installed/removed at the bit level.

use mytilus_sys::ctypes::{c_int, c_long, c_ulong};
use mytilus_sys::errno_raw::EINVAL;

// Force-link mytilus-errno: sigaddset/sigdelset write errno on validation
// failure, which goes through `__errno_location`.
extern crate mytilus_errno;

unsafe extern "C" {
    fn __errno_location() -> *mut c_int;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum signal number + 1, per `arch/aarch64/bits/signal.h`.
pub const _NSIG: c_int = 65;

/// `NSIG` (POSIX-spelled alias for `_NSIG`).
pub const NSIG: c_int = _NSIG;

/// Number of `c_ulong` words in `sigset_t::__bits`. On LP64 with
/// `c_ulong = u64` this is 16 (128 bytes / 8 bytes per word).
pub const SIGSET_NWORDS: usize = 128 / core::mem::size_of::<c_ulong>();

/// Number of `c_ulong` words actually used by signal bits — `_NSIG/8/sizeof(long)`.
/// On LP64 with `_NSIG = 65` this is 1 (only `__bits[0]` carries real data;
/// the rest of the struct is reserved padding).
const SST_SIZE: usize = (_NSIG as usize) / 8 / core::mem::size_of::<c_ulong>();

// ---------------------------------------------------------------------------
// sigset_t
// ---------------------------------------------------------------------------

/// `sigset_t` — a 128-byte signal-mask bitset matching the kernel ABI.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct sigset_t {
    pub __bits: [c_ulong; SIGSET_NWORDS],
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Bit position for `sig` within `__bits[s/64]`. `s = sig - 1` per upstream.
#[inline]
fn bit_index(sig: c_int) -> (usize, c_ulong) {
    let s = (sig - 1) as usize;
    let word = s / (8 * core::mem::size_of::<c_ulong>());
    let bit = (1 as c_ulong) << (s & (8 * core::mem::size_of::<c_ulong>() - 1));
    (word, bit)
}

/// Reject signals out of range or in the libc-reserved 32..=34 range.
/// Mirrors the validation in upstream sigaddset/sigdelset:
/// `if (s >= _NSIG-1 || sig-32U < 3) errno=EINVAL`.
#[inline]
fn validate_sig(sig: c_int) -> bool {
    let s = (sig - 1) as c_uint_helper;
    let nsig_m1 = (_NSIG - 1) as c_uint_helper;
    let sig_minus_32 = (sig as c_uint_helper).wrapping_sub(32);
    s >= nsig_m1 || sig_minus_32 < 3
}

/// `unsigned int` view we use only for the validation arithmetic above
/// (matches upstream's `unsigned s = sig-1` / `sig-32U` semantics).
type c_uint_helper = u32;

/// Set errno to EINVAL and return -1 — small helper used by add/del.
#[inline]
unsafe fn set_einval_return_minus1() -> c_int {
    // SAFETY: __errno_location is contractually a valid TLS pointer.
    unsafe {
        *__errno_location() = EINVAL;
    }
    -1
}

// ---------------------------------------------------------------------------
// sigemptyset / sigfillset
// ---------------------------------------------------------------------------

/// `int sigemptyset(sigset_t *set)` — clear every signal bit.
///
/// On LP64 with `_NSIG = 65` only `__bits[0]` holds real signals; upstream
/// only zeroes that one word and leaves the rest of the struct unspecified
/// (callers must always zero-init themselves before passing). We do the
/// same — bit-identical to upstream's `sigemptyset.c` for our target.
///
/// # Safety
/// `set` must point to a writable `sigset_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigemptyset(set: *mut sigset_t) -> c_int {
    // SAFETY: caller-provided pointer is asserted writable.
    unsafe {
        (*set).__bits[0] = 0;
    }
    // Branches `sizeof(long)==4 || _NSIG > 65` and `sizeof(long)==4 && _NSIG > 65`
    // upstream are both false on LP64 + _NSIG=65 — no other word touched.
    0
}

/// `int sigfillset(sigset_t *set)` — fill every non-reserved signal bit.
///
/// The constant `0xfffffffc7fffffff` masks out bits 31, 32, 33 (signals 32,
/// 33, 34) which musl reserves internally:
/// - signal 32 = `SIGCANCEL` (pthread_cancel)
/// - signal 33 = `SIGSYNCCALL` (synchronous call broadcast)
/// - signal 34 = reserved for `setxid` / future use
///
/// Bit identity with upstream: this is the LP64 branch of `sigfillset.c`.
///
/// # Safety
/// `set` must point to a writable `sigset_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigfillset(set: *mut sigset_t) -> c_int {
    // SAFETY: caller-provided pointer is asserted writable.
    unsafe {
        (*set).__bits[0] = 0xfffffffc_7fffffff;
    }
    // _NSIG > 65 path is dead on aarch64.
    0
}

// ---------------------------------------------------------------------------
// sigaddset / sigdelset
// ---------------------------------------------------------------------------

/// `int sigaddset(sigset_t *set, int sig)`.
///
/// # Safety
/// `set` must point to a writable `sigset_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigaddset(set: *mut sigset_t, sig: c_int) -> c_int {
    if validate_sig(sig) {
        // SAFETY: errno write only.
        return unsafe { set_einval_return_minus1() };
    }
    let (word, bit) = bit_index(sig);
    // SAFETY: caller-provided pointer is asserted writable; word index is
    // bounded by validate_sig (s < _NSIG-1 = 64; word = s/64 ∈ {0}).
    unsafe {
        (*set).__bits[word] |= bit;
    }
    0
}

/// `int sigdelset(sigset_t *set, int sig)`.
///
/// # Safety
/// `set` must point to a writable `sigset_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigdelset(set: *mut sigset_t, sig: c_int) -> c_int {
    if validate_sig(sig) {
        // SAFETY: errno write only.
        return unsafe { set_einval_return_minus1() };
    }
    let (word, bit) = bit_index(sig);
    // SAFETY: see sigaddset.
    unsafe {
        (*set).__bits[word] &= !bit;
    }
    0
}

// ---------------------------------------------------------------------------
// sigismember
// ---------------------------------------------------------------------------

/// `int sigismember(const sigset_t *set, int sig)`.
///
/// Out-of-range `sig` returns 0 (no errno). Unlike `sigaddset`/`sigdelset`,
/// querying a reserved signal (32..=34) is allowed.
///
/// # Safety
/// `set` must point to a readable `sigset_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigismember(set: *const sigset_t, sig: c_int) -> c_int {
    let s = (sig - 1) as c_uint_helper;
    if s >= (_NSIG - 1) as c_uint_helper {
        return 0;
    }
    let (word, bit) = bit_index(sig);
    // SAFETY: caller-provided pointer is asserted readable; bounded index.
    let raw = unsafe { (*set).__bits[word] };
    ((raw & bit) != 0) as c_int
}

// ---------------------------------------------------------------------------
// sigorset / sigandset / sigisemptyset
// ---------------------------------------------------------------------------

/// `int sigorset(sigset_t *dest, const sigset_t *left, const sigset_t *right)`
/// — bitwise OR over the `_NSIG/8/sizeof(long)` real signal words.
///
/// # Safety
/// All three pointers must reference valid `sigset_t`s; they may alias.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigorset(
    dest: *mut sigset_t,
    left: *const sigset_t,
    right: *const sigset_t,
) -> c_int {
    // SAFETY: caller asserts pointers are valid; we touch only the first
    // SST_SIZE words (= 1 on LP64+_NSIG=65), matching upstream.
    unsafe {
        for i in 0..SST_SIZE {
            (*dest).__bits[i] = (*left).__bits[i] | (*right).__bits[i];
        }
    }
    0
}

/// `int sigandset(sigset_t *dest, const sigset_t *left, const sigset_t *right)`
/// — bitwise AND over the real signal words.
///
/// # Safety
/// See [`sigorset`].
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigandset(
    dest: *mut sigset_t,
    left: *const sigset_t,
    right: *const sigset_t,
) -> c_int {
    // SAFETY: see sigorset.
    unsafe {
        for i in 0..SST_SIZE {
            (*dest).__bits[i] = (*left).__bits[i] & (*right).__bits[i];
        }
    }
    0
}

/// `int sigisemptyset(const sigset_t *set)` — 1 if no signals are set, else 0.
///
/// # Safety
/// `set` must point to a readable `sigset_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn sigisemptyset(set: *const sigset_t) -> c_int {
    // SAFETY: caller asserts pointer is valid; we touch only the first
    // SST_SIZE words.
    unsafe {
        for i in 0..SST_SIZE {
            if (*set).__bits[i] != 0 {
                return 0;
            }
        }
    }
    1
}

// `c_long` is unused now but pulled in by the file's `use` block; reference
// it once so a future contributor doesn't trip the unused-import warning when
// the imports get auto-cleaned.
const _: () = {
    let _: c_long = 0;
};

#[cfg(test)]
mod tests {
    use core::mem::{align_of, size_of};

    use super::*;

    fn empty_set() -> sigset_t {
        sigset_t {
            __bits: [0; SIGSET_NWORDS],
        }
    }

    fn errno() -> c_int {
        // SAFETY: __errno_location returns a per-thread valid pointer.
        unsafe { *__errno_location() }
    }

    fn clear_errno() {
        // SAFETY: per-thread pointer is writable.
        unsafe {
            *__errno_location() = 0;
        }
    }

    // ---- struct layout ------------------------------------------------

    #[test]
    fn sigset_layout_matches_kernel_abi() {
        // 128 bytes, 8-byte aligned (the kernel definition is fixed; any
        // drift here silently corrupts every signal-related syscall).
        assert_eq!(size_of::<sigset_t>(), 128);
        assert_eq!(align_of::<sigset_t>(), 8);
        assert_eq!(SIGSET_NWORDS, 16);
        assert_eq!(SST_SIZE, 1); // LP64 + _NSIG=65 specialization
    }

    // ---- sigemptyset / sigfillset -------------------------------------

    #[test]
    fn sigemptyset_zeroes_real_word() {
        let mut s = sigset_t {
            __bits: [0xdead_beef_dead_beef; SIGSET_NWORDS],
        };
        // SAFETY: stack-local sigset.
        let r = unsafe { sigemptyset(&mut s) };
        assert_eq!(r, 0);
        assert_eq!(s.__bits[0], 0);
        // Upstream leaves __bits[1..] alone on LP64+_NSIG=65 — verify we
        // mirror that (even though callers shouldn't rely on it).
        assert_eq!(s.__bits[1], 0xdead_beef_dead_beef);
    }

    #[test]
    fn sigfillset_excludes_reserved() {
        let mut s = empty_set();
        // SAFETY: stack-local sigset.
        let r = unsafe { sigfillset(&mut s) };
        assert_eq!(r, 0);
        assert_eq!(s.__bits[0], 0xfffffffc_7fffffff);
        // Spot-check: signals 32, 33, 34 are NOT set; signals 1, 31, 35, 64 ARE.
        // SAFETY: stack-local; sigismember reads.
        unsafe {
            assert_eq!(sigismember(&s, 1), 1);
            assert_eq!(sigismember(&s, 31), 1);
            assert_eq!(sigismember(&s, 32), 0);
            assert_eq!(sigismember(&s, 33), 0);
            assert_eq!(sigismember(&s, 34), 0);
            assert_eq!(sigismember(&s, 35), 1);
            assert_eq!(sigismember(&s, 64), 1);
        }
    }

    // ---- sigaddset / sigdelset / sigismember --------------------------

    #[test]
    fn sigaddset_then_ismember_round_trip() {
        let mut s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            assert_eq!(sigaddset(&mut s, 1), 0);
            assert_eq!(sigaddset(&mut s, 7), 0);
            assert_eq!(sigaddset(&mut s, 64), 0);
            assert_eq!(sigismember(&s, 1), 1);
            assert_eq!(sigismember(&s, 2), 0);
            assert_eq!(sigismember(&s, 7), 1);
            assert_eq!(sigismember(&s, 64), 1);
        }
    }

    #[test]
    fn sigdelset_clears_specific_bit() {
        let mut s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            sigaddset(&mut s, 5);
            sigaddset(&mut s, 6);
            assert_eq!(sigdelset(&mut s, 5), 0);
            assert_eq!(sigismember(&s, 5), 0);
            assert_eq!(sigismember(&s, 6), 1);
        }
    }

    #[test]
    fn sigaddset_rejects_reserved_signals() {
        let mut s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            for sig in [32, 33, 34] {
                clear_errno();
                assert_eq!(sigaddset(&mut s, sig), -1, "sig={sig}");
                assert_eq!(errno(), EINVAL);
            }
        }
    }

    #[test]
    fn sigdelset_rejects_reserved_signals() {
        let mut s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            for sig in [32, 33, 34] {
                clear_errno();
                assert_eq!(sigdelset(&mut s, sig), -1, "sig={sig}");
                assert_eq!(errno(), EINVAL);
            }
        }
    }

    #[test]
    fn sigaddset_rejects_out_of_range() {
        let mut s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            for sig in [-1, 0, 65, 100, 1000] {
                clear_errno();
                assert_eq!(sigaddset(&mut s, sig), -1, "sig={sig}");
                assert_eq!(errno(), EINVAL);
            }
        }
    }

    #[test]
    fn sigismember_allows_query_of_reserved_signals() {
        // Unlike add/del, sigismember can be asked about reserved signals.
        // It returns whatever bit is in the underlying word — even though
        // sigaddset would never have set it.
        let mut s = empty_set();
        s.__bits[0] = 1u64 << 31; // signal 32 bit
                                  // SAFETY: stack-local; sigismember reads.
        unsafe {
            assert_eq!(sigismember(&s, 32), 1);
        }
    }

    #[test]
    fn sigismember_returns_zero_for_out_of_range() {
        let s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            assert_eq!(sigismember(&s, 0), 0);
            assert_eq!(sigismember(&s, 65), 0);
            assert_eq!(sigismember(&s, 1000), 0);
            assert_eq!(sigismember(&s, -1), 0);
        }
    }

    // ---- sigorset / sigandset -----------------------------------------

    #[test]
    fn sigorset_unions() {
        let mut a = empty_set();
        let mut b = empty_set();
        let mut dst = empty_set();
        // SAFETY: stack-local.
        unsafe {
            sigaddset(&mut a, 1);
            sigaddset(&mut a, 5);
            sigaddset(&mut b, 5);
            sigaddset(&mut b, 10);
            assert_eq!(sigorset(&mut dst, &a, &b), 0);
            assert_eq!(sigismember(&dst, 1), 1);
            assert_eq!(sigismember(&dst, 5), 1);
            assert_eq!(sigismember(&dst, 10), 1);
            assert_eq!(sigismember(&dst, 2), 0);
        }
    }

    #[test]
    fn sigandset_intersects() {
        let mut a = empty_set();
        let mut b = empty_set();
        let mut dst = empty_set();
        // SAFETY: stack-local.
        unsafe {
            sigaddset(&mut a, 1);
            sigaddset(&mut a, 5);
            sigaddset(&mut b, 5);
            sigaddset(&mut b, 10);
            assert_eq!(sigandset(&mut dst, &a, &b), 0);
            assert_eq!(sigismember(&dst, 1), 0);
            assert_eq!(sigismember(&dst, 5), 1);
            assert_eq!(sigismember(&dst, 10), 0);
        }
    }

    // ---- sigisemptyset ------------------------------------------------

    #[test]
    fn sigisemptyset_reports_correctly() {
        let mut s = empty_set();
        // SAFETY: stack-local.
        unsafe {
            assert_eq!(sigisemptyset(&s), 1);
            sigaddset(&mut s, 7);
            assert_eq!(sigisemptyset(&s), 0);
            sigdelset(&mut s, 7);
            assert_eq!(sigisemptyset(&s), 1);
        }
    }
}
