//! `memcpy` / `memmove` / `memset` / `memcmp`.
//!
//! Two implementations live here:
//!   - **Cross target** (aarch64-linux-mytilus): the canonical `memcpy` and
//!     `memset` symbols are defined by hand-tuned aarch64 assembly ported
//!     verbatim from upstream musl (`asm/{memcpy,memset}.S`). `memmove` and
//!     `memcmp` use the byte-loop Rust impl below — upstream doesn't ship
//!     tuned aarch64 versions of those, so the byte loops are correct and
//!     "good enough" until profiling demands otherwise.
//!   - **Host** (macOS / x86_64-linux for tests + IDE): the byte-loop Rust
//!     impls below are the only thing built. They're cfg-gated to
//!     `not(target_env = "musl")` so they don't conflict with the asm
//!     symbols on cross. Tests reach them via the Rust path
//!     `crate::mem::{memcpy, memmove, memset, memcmp}`.
//!
//! TODO(perf): port a tuned aarch64 `memmove` / `memcmp` if profiling
//! shows them on a hot path. Upstream's generic C versions (used on
//! arches without dedicated asm) use a SWAR / word-stride scheme; there
//! are no upstream `aarch64/{memmove,memcmp}.S` files to crib from.

use mytilus_sys::ctypes::{c_int, c_void, size_t};

// ---------------------------------------------------------------------------
// Cross target: pull in the upstream aarch64 asm. Both `memcpy` and `memset`
// are defined as `.global` symbols there and become the canonical export.
// ---------------------------------------------------------------------------

#[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "musl"))]
core::arch::global_asm!(include_str!("../asm/memcpy.S"));

#[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "musl"))]
core::arch::global_asm!(include_str!("../asm/memset.S"));

// Rust callers in this crate (or downstream Rust users on cross) reach the
// asm-defined symbols via these declarations.
#[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "musl"))]
unsafe extern "C" {
    pub fn memcpy(dest: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
    pub fn memset(dest: *mut c_void, c: c_int, n: size_t) -> *mut c_void;
}

// ---------------------------------------------------------------------------
// Host: byte-loop Rust impl. Mangled (no `#[no_mangle]`) so it doesn't shadow
// libsystem's `memcpy`/`memset` in test binaries.
// ---------------------------------------------------------------------------

/// `void *memcpy(void *restrict dest, const void *restrict src, size_t n)`
///
/// # Safety
/// `dest` and `src` must each point to at least `n` writable / readable
/// bytes, and the regions must not overlap. Use `memmove` for overlap.
#[cfg(not(target_env = "musl"))]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;
    // SAFETY: caller guarantees both pointers are valid for n bytes and
    // non-overlapping. Each iteration is a single byte read/write to
    // disjoint addresses.
    unsafe {
        let mut i: usize = 0;
        while i < n {
            *d.add(i) = *s.add(i);
            i += 1;
        }
    }
    dest
}

/// `void *memmove(void *dest, const void *src, size_t n)`
///
/// Tolerates overlap by copying forward when `dest <= src` and backward
/// when `dest > src`.
///
/// # Safety
/// Both pointers must be valid for `n` bytes; overlap is permitted.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;
    // SAFETY: caller guarantees both pointers are valid for n bytes;
    // overlap is handled by choosing copy direction based on relative
    // address, matching the upstream behavior.
    unsafe {
        if (d as usize) < (s as usize) {
            let mut i: usize = 0;
            while i < n {
                *d.add(i) = *s.add(i);
                i += 1;
            }
        } else if (d as usize) > (s as usize) {
            let mut i = n;
            while i > 0 {
                i -= 1;
                *d.add(i) = *s.add(i);
            }
        }
    }
    dest
}

/// `void *memset(void *dest, int c, size_t n)`
///
/// Writes the low byte of `c` into the first `n` bytes of `dest`.
///
/// # Safety
/// `dest` must point to at least `n` writable bytes.
#[cfg(not(target_env = "musl"))]
pub unsafe extern "C" fn memset(dest: *mut c_void, c: c_int, n: size_t) -> *mut c_void {
    let d = dest as *mut u8;
    let byte = c as u8;
    // SAFETY: caller guarantees dest is valid for n bytes.
    unsafe {
        let mut i: usize = 0;
        while i < n {
            *d.add(i) = byte;
            i += 1;
        }
    }
    dest
}

/// `int memcmp(const void *vl, const void *vr, size_t n)`
///
/// Returns the difference of the first mismatched byte (as unsigned char),
/// or 0 if the regions are byte-equal.
///
/// # Safety
/// Both pointers must point to at least `n` readable bytes.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn memcmp(vl: *const c_void, vr: *const c_void, n: size_t) -> c_int {
    let l = vl as *const u8;
    let r = vr as *const u8;
    // SAFETY: caller guarantees both pointers are valid for n bytes.
    unsafe {
        let mut i: usize = 0;
        while i < n {
            let a = *l.add(i);
            let b = *r.add(i);
            if a != b {
                return a as c_int - b as c_int;
            }
            i += 1;
        }
    }
    0
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    // ---- memcpy ----------------------------------------------------------

    #[test]
    fn memcpy_basic() {
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = [0u8; 8];
        // SAFETY: disjoint stack arrays.
        let ret = unsafe {
            memcpy(
                dst.as_mut_ptr() as *mut c_void,
                src.as_ptr() as *const c_void,
                8,
            )
        };
        assert_eq!(ret, dst.as_mut_ptr() as *mut c_void);
        assert_eq!(dst, src);
    }

    #[test]
    fn memcpy_zero_length_is_noop() {
        let src = [42u8; 4];
        let mut dst = [0u8; 4];
        // SAFETY: disjoint; n=0 means we read/write nothing.
        unsafe {
            memcpy(
                dst.as_mut_ptr() as *mut c_void,
                src.as_ptr() as *const c_void,
                0,
            );
        }
        assert_eq!(dst, [0u8; 4]);
    }

    #[test]
    fn memcpy_partial() {
        let src = [9u8; 16];
        let mut dst = [0u8; 16];
        // SAFETY: disjoint; copy 5 of 16.
        unsafe {
            memcpy(
                dst.as_mut_ptr() as *mut c_void,
                src.as_ptr() as *const c_void,
                5,
            );
        }
        assert_eq!(&dst[..5], &[9, 9, 9, 9, 9]);
        assert_eq!(&dst[5..], &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    // ---- memmove ---------------------------------------------------------

    #[test]
    fn memmove_no_overlap() {
        let src = [1u8, 2, 3, 4, 5];
        let mut dst = [0u8; 5];
        // SAFETY: disjoint.
        unsafe {
            memmove(
                dst.as_mut_ptr() as *mut c_void,
                src.as_ptr() as *const c_void,
                5,
            );
        }
        assert_eq!(dst, src);
    }

    #[test]
    fn memmove_overlap_dst_after_src() {
        // Copy forward by 2: requires backward iteration to avoid clobbering.
        let mut buf = [1u8, 2, 3, 4, 5, 0, 0];
        // SAFETY: shifts the first 5 bytes to positions 2..7 within the same
        // buffer; memmove must detect the overlap and iterate backward.
        unsafe {
            let p = buf.as_mut_ptr();
            memmove(p.add(2) as *mut c_void, p as *const c_void, 5);
        }
        assert_eq!(buf, [1, 2, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn memmove_overlap_dst_before_src() {
        // Copy backward by 2: forward iteration is correct.
        let mut buf = [0u8, 0, 1, 2, 3, 4, 5];
        // SAFETY: shifts bytes 2..7 to positions 0..5 within the same buffer.
        unsafe {
            let p = buf.as_mut_ptr();
            memmove(p as *mut c_void, p.add(2) as *const c_void, 5);
        }
        assert_eq!(buf, [1, 2, 3, 4, 5, 4, 5]);
    }

    #[test]
    fn memmove_same_pointer() {
        let mut buf = [1u8, 2, 3, 4];
        // SAFETY: dest == src; should be a no-op (or a self-copy).
        unsafe {
            let p = buf.as_mut_ptr();
            memmove(p as *mut c_void, p as *const c_void, 4);
        }
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    // ---- memset ----------------------------------------------------------

    #[test]
    fn memset_basic() {
        let mut dst = [0u8; 8];
        // SAFETY: stack array of 8.
        let ret = unsafe { memset(dst.as_mut_ptr() as *mut c_void, 0xab, 8) };
        assert_eq!(ret, dst.as_mut_ptr() as *mut c_void);
        assert_eq!(dst, [0xab; 8]);
    }

    #[test]
    fn memset_uses_low_byte_only() {
        // memset takes int but only writes the low 8 bits. 0x1234 should
        // produce 0x34 across the buffer.
        let mut dst = [0u8; 4];
        // SAFETY: stack array of 4.
        unsafe {
            memset(dst.as_mut_ptr() as *mut c_void, 0x1234, 4);
        }
        assert_eq!(dst, [0x34, 0x34, 0x34, 0x34]);
    }

    #[test]
    fn memset_zero_length_is_noop() {
        let mut dst = [9u8; 4];
        // SAFETY: stack array; n=0.
        unsafe {
            memset(dst.as_mut_ptr() as *mut c_void, 0, 0);
        }
        assert_eq!(dst, [9, 9, 9, 9]);
    }

    // ---- memcmp ----------------------------------------------------------

    #[test]
    fn memcmp_equal() {
        let a = [1u8, 2, 3, 4];
        let b = [1u8, 2, 3, 4];
        // SAFETY: stack arrays.
        let r = unsafe { memcmp(a.as_ptr() as *const c_void, b.as_ptr() as *const c_void, 4) };
        assert_eq!(r, 0);
    }

    #[test]
    fn memcmp_first_byte_differs() {
        let a = [5u8, 2, 3];
        let b = [3u8, 2, 3];
        // SAFETY: stack arrays.
        let r = unsafe { memcmp(a.as_ptr() as *const c_void, b.as_ptr() as *const c_void, 3) };
        assert_eq!(r, 2);
    }

    #[test]
    fn memcmp_treats_bytes_as_unsigned() {
        // 0xFF (255) > 0x01 (1), so the result is positive.
        let a = [0xffu8];
        let b = [0x01u8];
        // SAFETY: stack arrays of 1.
        let r = unsafe { memcmp(a.as_ptr() as *const c_void, b.as_ptr() as *const c_void, 1) };
        assert_eq!(r, 254);
    }

    #[test]
    fn memcmp_stops_at_first_mismatch() {
        // Only the first 2 bytes match; the third differs but is past n=2.
        let a = [1u8, 2, 99];
        let b = [1u8, 2, 0];
        // SAFETY: stack arrays.
        let r = unsafe { memcmp(a.as_ptr() as *const c_void, b.as_ptr() as *const c_void, 2) };
        assert_eq!(r, 0);
    }

    #[test]
    fn memcmp_zero_length_is_zero() {
        let a = [1u8];
        let b = [2u8];
        // SAFETY: n=0, no reads.
        let r = unsafe { memcmp(a.as_ptr() as *const c_void, b.as_ptr() as *const c_void, 0) };
        assert_eq!(r, 0);
    }
}
