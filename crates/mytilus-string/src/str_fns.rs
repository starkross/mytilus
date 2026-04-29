//! `str*` and friends: length, compare, search, copy, concat, plus
//! `memchr`/`memrchr` (which logically belong with the `mem*` family but
//! are needed by `strrchr` here, so kept in this module).
//!
//! Also `strerror_r` / `__xpg_strerror_r` (upstream lives in
//! `src/string/strerror_r.c`); wires up `mytilus_errno::strerror_str`.
//!
//! TODO(perf): same story as `mem.rs` — these are byte-loop ports, intended
//! to be replaced (where it matters) by tuned aarch64 routines later. The
//! upstream `__GNUC__` block uses a SWAR `HASZERO` trick to scan a word at
//! a time; we deliberately don't port that yet — correctness first, perf via
//! `.S` files when the asm-build plumbing lands.

use mytilus_sys::ctypes::{c_char, c_int, c_void, size_t};

// ---------------------------------------------------------------------------
// Length: strlen, strnlen
// ---------------------------------------------------------------------------

/// `size_t strlen(const char *s)`
///
/// # Safety
/// `s` must point to a NUL-terminated C string.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strlen(s: *const c_char) -> size_t {
    // SAFETY: caller guarantees s is NUL-terminated; we read forward until
    // the first 0 byte.
    unsafe {
        let mut n: size_t = 0;
        while *s.add(n) != 0 {
            n += 1;
        }
        n
    }
}

/// `size_t strnlen(const char *s, size_t n)`
///
/// # Safety
/// `s` must point to at least `n` readable bytes.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strnlen(s: *const c_char, n: size_t) -> size_t {
    // SAFETY: caller guarantees s is valid for n bytes; bounded scan.
    unsafe {
        let mut i: size_t = 0;
        while i < n && *s.add(i) != 0 {
            i += 1;
        }
        i
    }
}

// ---------------------------------------------------------------------------
// Memory search: memchr, memrchr (+ __memrchr internal alias)
// ---------------------------------------------------------------------------

/// `void *memchr(const void *src, int c, size_t n)`
///
/// # Safety
/// `src` must be valid for `n` bytes.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn memchr(src: *const c_void, c: c_int, n: size_t) -> *mut c_void {
    let s = src as *const u8;
    let target = c as u8;
    // SAFETY: caller-provided buffer of n bytes.
    unsafe {
        let mut i: size_t = 0;
        while i < n {
            if *s.add(i) == target {
                return s.add(i) as *mut c_void;
            }
            i += 1;
        }
    }
    core::ptr::null_mut()
}

/// Internal name; upstream is `weak_alias(__memrchr, memrchr)`.
///
/// # Safety
/// `src` must be valid for `n` bytes.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn __memrchr(src: *const c_void, c: c_int, n: size_t) -> *mut c_void {
    let s = src as *const u8;
    let target = c as u8;
    // SAFETY: caller-provided buffer of n bytes; we walk backward from index
    // n-1 down to 0.
    unsafe {
        let mut i = n;
        while i > 0 {
            i -= 1;
            if *s.add(i) == target {
                return s.add(i) as *mut c_void;
            }
        }
    }
    core::ptr::null_mut()
}

/// # Safety
/// See `__memrchr`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn memrchr(src: *const c_void, c: c_int, n: size_t) -> *mut c_void {
    // SAFETY: forwarded.
    unsafe { __memrchr(src, c, n) }
}

// ---------------------------------------------------------------------------
// String search: strchrnul (+ __ alias), strchr, strrchr
// ---------------------------------------------------------------------------

/// Internal name; upstream is `weak_alias(__strchrnul, strchrnul)`.
/// Returns a pointer to the first byte equal to `c`, or to the terminating
/// NUL if no match. Searching for `c == 0` returns a pointer to the NUL.
///
/// # Safety
/// `s` must be NUL-terminated.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn __strchrnul(s: *const c_char, c: c_int) -> *mut c_char {
    let target = c as u8;
    // SAFETY: caller guarantees s is NUL-terminated.
    unsafe {
        // Special case: c == 0 means "find the NUL", same as s + strlen(s).
        if target == 0 {
            return s.add(strlen(s)) as *mut c_char;
        }
        let mut p = s;
        while *p != 0 && *p != target {
            p = p.add(1);
        }
        p as *mut c_char
    }
}

/// # Safety
/// See `__strchrnul`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strchrnul(s: *const c_char, c: c_int) -> *mut c_char {
    // SAFETY: forwarded.
    unsafe { __strchrnul(s, c) }
}

/// `char *strchr(const char *s, int c)`
///
/// # Safety
/// `s` must be NUL-terminated.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    // SAFETY: __strchrnul returns a pointer into the same buffer; if it
    // matched the requested byte, return it; otherwise NULL.
    unsafe {
        let r = __strchrnul(s, c);
        if *r == c as u8 {
            r
        } else {
            core::ptr::null_mut()
        }
    }
}

/// `char *strrchr(const char *s, int c)`
///
/// # Safety
/// `s` must be NUL-terminated.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strrchr(s: *const c_char, c: c_int) -> *mut c_char {
    // SAFETY: scan the entire string including its terminator (the +1) so
    // strrchr(s, 0) finds the NUL.
    unsafe {
        let len = strlen(s);
        __memrchr(s as *const c_void, c, len + 1) as *mut c_char
    }
}

// ---------------------------------------------------------------------------
// Compare: strcmp, strncmp
// ---------------------------------------------------------------------------

/// `int strcmp(const char *l, const char *r)`
///
/// # Safety
/// Both pointers must be NUL-terminated.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strcmp(l: *const c_char, r: *const c_char) -> c_int {
    // SAFETY: caller guarantees both strings are NUL-terminated; we walk
    // until we find a difference or hit a terminator.
    unsafe {
        let mut i: usize = 0;
        loop {
            let a = *l.add(i);
            let b = *r.add(i);
            if a != b || a == 0 {
                return a as c_int - b as c_int;
            }
            i += 1;
        }
    }
}

/// `int strncmp(const char *l, const char *r, size_t n)`
///
/// # Safety
/// Both pointers must be valid for `min(n, strlen+1)` bytes.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strncmp(l: *const c_char, r: *const c_char, mut n: size_t) -> c_int {
    if n == 0 {
        return 0;
    }
    // Upstream: `if (!n--) return 0;` then loop while *l && *r && n && *l==*r.
    n -= 1;
    let mut l = l;
    let mut r = r;
    // SAFETY: caller guarantees both buffers are valid for the bytes we touch.
    unsafe {
        while *l != 0 && *r != 0 && n != 0 && *l == *r {
            l = l.add(1);
            r = r.add(1);
            n -= 1;
        }
        *l as c_int - *r as c_int
    }
}

// ---------------------------------------------------------------------------
// Copy: stpcpy, strcpy, stpncpy, strncpy
// ---------------------------------------------------------------------------

/// Internal name; upstream is `weak_alias(__stpcpy, stpcpy)`. Returns a
/// pointer to the terminating NUL written to `d`.
///
/// # Safety
/// `d` must be writable for `strlen(s) + 1` bytes; regions must not overlap.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn __stpcpy(mut d: *mut c_char, mut s: *const c_char) -> *mut c_char {
    // SAFETY: caller guarantees the destination is large enough.
    unsafe {
        loop {
            *d = *s;
            if *s == 0 {
                return d;
            }
            d = d.add(1);
            s = s.add(1);
        }
    }
}

/// # Safety
/// See `__stpcpy`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn stpcpy(d: *mut c_char, s: *const c_char) -> *mut c_char {
    // SAFETY: forwarded.
    unsafe { __stpcpy(d, s) }
}

/// `char *strcpy(char *restrict dest, const char *restrict src)`
///
/// # Safety
/// See `__stpcpy`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strcpy(d: *mut c_char, s: *const c_char) -> *mut c_char {
    // SAFETY: forwarded.
    unsafe {
        __stpcpy(d, s);
    }
    d
}

/// Internal name; upstream is `weak_alias(__stpncpy, stpncpy)`.
///
/// Returns a pointer to the first NUL byte written if the source ended within
/// `n` bytes, or `d + n` if it didn't.
///
/// # Safety
/// `d` must be writable for `n` bytes; `s` must be readable up to the first
/// NUL or `n` bytes, whichever comes first.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn __stpncpy(
    mut d: *mut c_char,
    mut s: *const c_char,
    mut n: size_t,
) -> *mut c_char {
    // SAFETY: caller guarantees the buffers are valid for the bytes we touch.
    unsafe {
        while n > 0 {
            *d = *s;
            if *s == 0 {
                break;
            }
            d = d.add(1);
            s = s.add(1);
            n -= 1;
        }
        // Zero-fill the remaining `n` bytes (upstream calls `memset(d, 0, n)`;
        // inlining the loop avoids a circular symbol-resolution dance under
        // cfg(test) where memset is mangled).
        let mut i: size_t = 0;
        while i < n {
            *d.add(i) = 0;
            i += 1;
        }
        d
    }
}

/// # Safety
/// See `__stpncpy`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn stpncpy(d: *mut c_char, s: *const c_char, n: size_t) -> *mut c_char {
    // SAFETY: forwarded.
    unsafe { __stpncpy(d, s, n) }
}

/// `char *strncpy(char *restrict d, const char *restrict s, size_t n)`
///
/// # Safety
/// See `__stpncpy`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strncpy(d: *mut c_char, s: *const c_char, n: size_t) -> *mut c_char {
    // SAFETY: forwarded.
    unsafe {
        __stpncpy(d, s, n);
    }
    d
}

// ---------------------------------------------------------------------------
// Concat: strcat
// ---------------------------------------------------------------------------

/// `char *strcat(char *restrict dest, const char *restrict src)`
///
/// # Safety
/// `dest` must be NUL-terminated and have room for `strlen(src) + 1` more
/// bytes after its existing content.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strcat(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    // SAFETY: caller guarantees the destination is sized to fit the
    // concatenation including the new terminator.
    unsafe {
        let len = strlen(dest);
        strcpy(dest.add(len), src);
    }
    dest
}

// ---------------------------------------------------------------------------
// strerror_r — wires mytilus_errno's table into the C ABI.
// ---------------------------------------------------------------------------

/// `int strerror_r(int err, char *buf, size_t buflen)`
///
/// Posix-spelled variant. Returns 0 on success, `ERANGE` if the message
/// didn't fit in the buffer (still copies as much as fits, NUL-terminating).
///
/// # Safety
/// `buf` must be writable for `buflen` bytes.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn strerror_r(err: c_int, buf: *mut c_char, buflen: size_t) -> c_int {
    // Use the Rust-level helper so we're not bouncing through the C symbol
    // dance (which would be ugly under cfg(test) where the symbols are
    // mangled).
    let bytes = mytilus_errno::strerror_str(err).to_bytes();
    let l = bytes.len();
    // SAFETY: caller guarantees buf is valid for buflen bytes; we either
    // copy the full message + NUL, or as much as fits + NUL and return ERANGE.
    unsafe {
        if l >= buflen {
            if buflen > 0 {
                let copy_n = buflen - 1;
                let mut i: size_t = 0;
                while i < copy_n {
                    *buf.add(i) = bytes[i] as c_char;
                    i += 1;
                }
                *buf.add(buflen - 1) = 0;
            }
            return mytilus_errno::ERANGE;
        }
        let mut i: size_t = 0;
        while i < l {
            *buf.add(i) = bytes[i] as c_char;
            i += 1;
        }
        *buf.add(l) = 0;
    }
    0
}

/// XSI/XPG variant: same signature and behavior as `strerror_r` here.
/// Upstream: `weak_alias(strerror_r, __xpg_strerror_r)`.
///
/// # Safety
/// See `strerror_r`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn __xpg_strerror_r(err: c_int, buf: *mut c_char, buflen: size_t) -> c_int {
    // SAFETY: forwarded.
    unsafe { strerror_r(err, buf, buflen) }
}

#[cfg(test)]
mod tests {
    use core::ffi::CStr;

    use super::*;

    fn cs(b: &[u8]) -> *const c_char {
        // CStr::from_bytes_with_nul validates exactly one trailing NUL.
        // Cast because std's c_char is i8 on macOS but our crate-wide
        // c_char (per mytilus-sys::ctypes) is always u8 (AArch64 PCS).
        CStr::from_bytes_with_nul(b).unwrap().as_ptr() as *const c_char
    }

    // ---- strlen / strnlen ---------------------------------------------

    #[test]
    fn strlen_basic() {
        // SAFETY: literal C strings; cs() validates NUL termination.
        unsafe {
            assert_eq!(strlen(cs(b"hello\0")), 5);
            assert_eq!(strlen(cs(b"\0")), 0);
        }
    }

    #[test]
    fn strnlen_bounded() {
        let s = cs(b"hello\0");
        // SAFETY: 6-byte buffer including NUL.
        unsafe {
            assert_eq!(strnlen(s, 3), 3); // bound hits before NUL
            assert_eq!(strnlen(s, 5), 5); // bound == length
            assert_eq!(strnlen(s, 100), 5); // NUL hits before bound
            assert_eq!(strnlen(s, 0), 0);
        }
    }

    // ---- memchr / memrchr ---------------------------------------------

    #[test]
    fn memchr_finds_or_null() {
        let buf = b"abcabc";
        // SAFETY: stack array of 6.
        unsafe {
            let p = memchr(buf.as_ptr() as *const c_void, b'c' as c_int, 6);
            assert_eq!(p, buf.as_ptr().add(2) as *mut c_void);
            let p = memchr(buf.as_ptr() as *const c_void, b'z' as c_int, 6);
            assert!(p.is_null());
        }
    }

    #[test]
    fn memrchr_finds_last() {
        let buf = b"abcabc";
        // SAFETY: stack array of 6.
        unsafe {
            let p = memrchr(buf.as_ptr() as *const c_void, b'c' as c_int, 6);
            assert_eq!(p, buf.as_ptr().add(5) as *mut c_void);
            let p = __memrchr(buf.as_ptr() as *const c_void, b'a' as c_int, 6);
            assert_eq!(p, buf.as_ptr().add(3) as *mut c_void);
        }
    }

    // ---- strchr / strrchr / strchrnul ---------------------------------

    #[test]
    fn strchr_finds_first() {
        let s = cs(b"hello\0");
        // SAFETY: NUL-terminated.
        unsafe {
            let p = strchr(s, b'l' as c_int);
            assert_eq!(p, s.add(2) as *mut c_char);
            assert!(strchr(s, b'z' as c_int).is_null());
            // Searching for NUL returns the position of the terminator.
            assert_eq!(strchr(s, 0), s.add(5) as *mut c_char);
        }
    }

    #[test]
    fn strchrnul_returns_terminator_on_miss() {
        let s = cs(b"abc\0");
        // SAFETY: NUL-terminated.
        unsafe {
            // Hit:
            assert_eq!(__strchrnul(s, b'b' as c_int), s.add(1) as *mut c_char);
            // Miss → returns the terminator, NOT NULL.
            assert_eq!(strchrnul(s, b'z' as c_int), s.add(3) as *mut c_char);
            // c=0 → returns terminator immediately.
            assert_eq!(__strchrnul(s, 0), s.add(3) as *mut c_char);
        }
    }

    #[test]
    fn strrchr_finds_last() {
        let s = cs(b"abcabc\0");
        // SAFETY: NUL-terminated.
        unsafe {
            let p = strrchr(s, b'a' as c_int);
            assert_eq!(p, s.add(3) as *mut c_char);
            let p = strrchr(s, b'c' as c_int);
            assert_eq!(p, s.add(5) as *mut c_char);
            // Includes terminator: strrchr(s, 0) returns position of NUL.
            assert_eq!(strrchr(s, 0), s.add(6) as *mut c_char);
            assert!(strrchr(s, b'z' as c_int).is_null());
        }
    }

    // ---- strcmp / strncmp ---------------------------------------------

    #[test]
    fn strcmp_orders() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strcmp(cs(b"abc\0"), cs(b"abc\0")), 0);
            assert!(strcmp(cs(b"abc\0"), cs(b"abd\0")) < 0);
            assert!(strcmp(cs(b"abd\0"), cs(b"abc\0")) > 0);
            // Different lengths: shorter is "less" if it's a prefix.
            assert!(strcmp(cs(b"ab\0"), cs(b"abc\0")) < 0);
            assert!(strcmp(cs(b"abc\0"), cs(b"ab\0")) > 0);
        }
    }

    #[test]
    fn strncmp_respects_n() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            // First 3 bytes equal.
            assert_eq!(strncmp(cs(b"abcdef\0"), cs(b"abcxyz\0"), 3), 0);
            // First 4 bytes differ at index 3.
            assert!(strncmp(cs(b"abcdef\0"), cs(b"abcxyz\0"), 4) < 0);
            // n=0 is always 0.
            assert_eq!(strncmp(cs(b"a\0"), cs(b"b\0"), 0), 0);
            // Stops at NUL even if n is large.
            assert_eq!(strncmp(cs(b"abc\0"), cs(b"abc\0"), 100), 0);
        }
    }

    // ---- strcpy / stpcpy ---------------------------------------------

    #[test]
    fn strcpy_copies_with_terminator() {
        let mut dst = [b'X'; 8];
        // SAFETY: dst is 8 bytes; src is "hello" (6 bytes incl NUL).
        let ret = unsafe { strcpy(dst.as_mut_ptr() as *mut c_char, cs(b"hello\0")) };
        assert_eq!(ret, dst.as_mut_ptr() as *mut c_char);
        assert_eq!(&dst[..6], b"hello\0");
        // Bytes past the NUL are untouched.
        assert_eq!(&dst[6..], b"XX");
    }

    #[test]
    fn stpcpy_returns_pointer_to_terminator() {
        let mut dst = [0u8; 8];
        // SAFETY: dst is 8 bytes; "hi\0" is 3 bytes; .add(2) is in bounds;
        // *end reads the NUL we just wrote.
        unsafe {
            let end = stpcpy(dst.as_mut_ptr() as *mut c_char, cs(b"hi\0"));
            assert_eq!(end, dst.as_mut_ptr().add(2) as *mut c_char);
            assert_eq!(*end, 0);
        }
    }

    // ---- strncpy / stpncpy --------------------------------------------

    #[test]
    fn strncpy_pads_with_zeros_when_short() {
        let mut dst = [b'X'; 8];
        // SAFETY: dst is 8 bytes; src is 3 chars + NUL.
        unsafe { strncpy(dst.as_mut_ptr() as *mut c_char, cs(b"hi\0"), 6) };
        // First 2 bytes = "hi", remaining 4 (in the n=6) are zero-padded.
        assert_eq!(&dst[..6], b"hi\0\0\0\0");
        // Bytes past n=6 are untouched.
        assert_eq!(&dst[6..], b"XX");
    }

    #[test]
    fn strncpy_truncates_when_long() {
        let mut dst = [b'X'; 8];
        // SAFETY: dst is 8 bytes; src is 5 chars + NUL.
        unsafe { strncpy(dst.as_mut_ptr() as *mut c_char, cs(b"hello\0"), 3) };
        // First 3 bytes = "hel"; NO terminator written when n < strlen(src).
        assert_eq!(&dst[..3], b"hel");
        assert_eq!(&dst[3..], b"XXXXX");
    }

    #[test]
    fn stpncpy_returns_first_nul_or_d_plus_n() {
        // Source shorter than n: returns pointer to first NUL.
        // SAFETY: dst is 8 bytes; .add(2) is in bounds.
        unsafe {
            let mut dst = [0u8; 8];
            let end = stpncpy(dst.as_mut_ptr() as *mut c_char, cs(b"hi\0"), 5);
            assert_eq!(end, dst.as_mut_ptr().add(2) as *mut c_char);
        }

        // Source longer than n: returns d+n (no NUL written within n bytes).
        // SAFETY: dst2 is 4 bytes; we only write 3; .add(3) is in bounds.
        unsafe {
            let mut dst2 = [b'X'; 4];
            let end2 = stpncpy(dst2.as_mut_ptr() as *mut c_char, cs(b"abcde\0"), 3);
            assert_eq!(end2, dst2.as_mut_ptr().add(3) as *mut c_char);
        }
    }

    // ---- strcat ------------------------------------------------------

    #[test]
    fn strcat_appends() {
        let mut dst = [0u8; 16];
        dst[..4].copy_from_slice(b"abc\0");
        // SAFETY: dst is 16 bytes; "abc" + "def" + NUL fits.
        unsafe { strcat(dst.as_mut_ptr() as *mut c_char, cs(b"def\0")) };
        assert_eq!(&dst[..7], b"abcdef\0");
    }

    // ---- strerror_r --------------------------------------------------

    #[test]
    fn strerror_r_writes_full_message() {
        let mut buf = [0u8; 64];
        // SAFETY: 64-byte buffer.
        let r = unsafe {
            strerror_r(
                mytilus_errno::EAGAIN,
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
            )
        };
        assert_eq!(r, 0);
        let msg = CStr::from_bytes_until_nul(&buf).unwrap();
        assert_eq!(msg.to_bytes(), b"Resource temporarily unavailable");
    }

    #[test]
    fn strerror_r_truncates_on_overflow() {
        let mut buf = [b'X'; 8];
        // SAFETY: 8-byte buffer; "Resource temporarily unavailable" is 32+
        // bytes, so we expect ERANGE plus a NUL-terminated truncation.
        let r = unsafe {
            strerror_r(
                mytilus_errno::EAGAIN,
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
            )
        };
        assert_eq!(r, mytilus_errno::ERANGE);
        // First 7 bytes of "Resource temporarily..." then a NUL at byte 7.
        assert_eq!(&buf[..7], b"Resourc");
        assert_eq!(buf[7], 0);
    }

    #[test]
    fn strerror_r_zero_length_returns_erange_only() {
        // When buflen == 0 we cannot write anything (not even a NUL); musl
        // returns ERANGE without touching the buffer.
        // SAFETY: buflen=0 means the buf pointer is never dereferenced.
        let r = unsafe { strerror_r(mytilus_errno::EAGAIN, core::ptr::null_mut(), 0) };
        assert_eq!(r, mytilus_errno::ERANGE);
    }
}
