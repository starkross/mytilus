//! Phase 3: search, tokenize, case-insensitive compare.
//!
//! Functions: `strstr`, `strspn`, `strcspn`, `strpbrk`, `strsep`, `strtok`,
//! `strtok_r`, `strcasecmp`, `strncasecmp` (+ the `*_l` locale-wrapper aliases).
//!
//! TODO(perf): `strstr` is the naive O(n*m) version. Upstream has a 4-way
//! switch on needle length that uses a 2/3/4-byte rolling-hash search, then
//! the Crochemore–Perrin Two-Way string-matching algorithm for ≥5-byte
//! needles. That's worth ~150 LOC of tricky code; deferred until needed.
//!
//! Case-folding for `strcasecmp`/`strncasecmp` goes through
//! `mytilus_locale::tolower`. The `_l` wrappers ignore their `locale_t`
//! argument because upstream musl's `__strcasecmp_l` does too — musl is C
//! locale only.

use core::sync::atomic::{AtomicPtr, Ordering};

use mytilus_locale::tolower;
use mytilus_sys::ctypes::{c_char, c_int, c_void, size_t};

use crate::str_fns::__strchrnul;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// 256-bit byteset for `strspn`/`strcspn`, allocated on the caller's stack.
type Byteset = [u64; 4];

#[inline]
fn byteset_set(bs: &mut Byteset, b: u8) {
    bs[(b as usize) / 64] |= 1u64 << (b as usize % 64);
}

#[inline]
fn byteset_test(bs: &Byteset, b: u8) -> bool {
    (bs[(b as usize) / 64] >> (b as usize % 64)) & 1 != 0
}

/// Case-fold a byte through `mytilus_locale::tolower` and return the byte.
/// Wraps the `c_int`→`c_int` C ABI in the form we need for byte comparisons.
#[inline]
fn fold(b: u8) -> u8 {
    tolower(b as c_int) as u8
}

// ---------------------------------------------------------------------------
// strstr  (naive O(n*m); TODO above)
// ---------------------------------------------------------------------------

/// `char *strstr(const char *h, const char *n)`
///
/// # Safety
/// Both `h` and `n` must be NUL-terminated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strstr(h: *const c_char, n: *const c_char) -> *mut c_char {
    // SAFETY: caller guarantees both strings are NUL-terminated; we walk
    // forward through h and, at each position, attempt to match n.
    unsafe {
        // Empty needle matches at h.
        if *n == 0 {
            return h as *mut c_char;
        }
        let mut h = h;
        while *h != 0 {
            let mut p = h;
            let mut q = n;
            while *p != 0 && *q != 0 && *p == *q {
                p = p.add(1);
                q = q.add(1);
            }
            if *q == 0 {
                return h as *mut c_char;
            }
            h = h.add(1);
        }
        core::ptr::null_mut()
    }
}

// ---------------------------------------------------------------------------
// strspn / strcspn / strpbrk
// ---------------------------------------------------------------------------

/// `size_t strspn(const char *s, const char *c)` — length of the prefix of
/// `s` consisting entirely of bytes from `c`.
///
/// # Safety
/// Both pointers must be NUL-terminated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strspn(s: *const c_char, c: *const c_char) -> size_t {
    // SAFETY: caller guarantees both strings are NUL-terminated.
    unsafe {
        if *c == 0 {
            return 0;
        }
        // Single-byte fast path: just count leading occurrences of c[0].
        if *c.add(1) == 0 {
            let mut p = s;
            while *p == *c {
                p = p.add(1);
            }
            return p.offset_from(s) as size_t;
        }

        let mut byteset: Byteset = [0; 4];
        let mut cp = c;
        while *cp != 0 {
            byteset_set(&mut byteset, *cp);
            cp = cp.add(1);
        }
        let mut sp = s;
        while *sp != 0 && byteset_test(&byteset, *sp) {
            sp = sp.add(1);
        }
        sp.offset_from(s) as size_t
    }
}

/// `size_t strcspn(const char *s, const char *c)` — length of the prefix of
/// `s` consisting of bytes NOT in `c`.
///
/// # Safety
/// Both pointers must be NUL-terminated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strcspn(s: *const c_char, c: *const c_char) -> size_t {
    // SAFETY: caller guarantees both strings are NUL-terminated.
    unsafe {
        // Empty c: every byte qualifies; equivalent to strlen(s).
        // Single-byte c: equivalent to (__strchrnul(s, c[0]) - s).
        if *c == 0 || *c.add(1) == 0 {
            return __strchrnul(s, *c as c_int).offset_from(s) as size_t;
        }

        let mut byteset: Byteset = [0; 4];
        let mut cp = c;
        while *cp != 0 {
            byteset_set(&mut byteset, *cp);
            cp = cp.add(1);
        }
        let mut sp = s;
        while *sp != 0 && !byteset_test(&byteset, *sp) {
            sp = sp.add(1);
        }
        sp.offset_from(s) as size_t
    }
}

/// `char *strpbrk(const char *s, const char *b)` — first byte of `s` that
/// appears in `b`, or NULL.
///
/// # Safety
/// Both pointers must be NUL-terminated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strpbrk(s: *const c_char, b: *const c_char) -> *mut c_char {
    // SAFETY: caller guarantees both strings are NUL-terminated; strcspn
    // returns the offset of the first match (or strlen(s) if none, in
    // which case *p will be NUL).
    unsafe {
        let p = s.add(strcspn(s, b));
        if *p != 0 {
            p as *mut c_char
        } else {
            core::ptr::null_mut()
        }
    }
}

// ---------------------------------------------------------------------------
// strsep, strtok, strtok_r
// ---------------------------------------------------------------------------

/// `char *strsep(char **str, const char *sep)` — POSIX/BSD tokenizer that
/// returns the next token in `*str` (which is updated to point past it),
/// writing a NUL over the separator. Treats consecutive separators as
/// empty tokens (unlike `strtok`).
///
/// # Safety
/// `str` must be a valid `**char`; if `*str` is non-NULL it must be
/// NUL-terminated and writable. `sep` must be NUL-terminated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strsep(str_: *mut *mut c_char, sep: *const c_char) -> *mut c_char {
    // SAFETY: caller-provided pointers; we mutate *str_ in place.
    unsafe {
        let s = *str_;
        if s.is_null() {
            return core::ptr::null_mut();
        }
        let end = s.add(strcspn(s, sep));
        let new_head = if *end != 0 {
            *end = 0;
            end.add(1)
        } else {
            core::ptr::null_mut()
        };
        *str_ = new_head;
        s
    }
}

/// `char *strtok_r(char *restrict s, const char *restrict sep, char **restrict p)`
///
/// # Safety
/// `s` and `sep` (both `*const c_char`-shaped) must be NUL-terminated when
/// non-NULL. `p` must be a valid `**char` whose pointee will be updated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strtok_r(
    mut s: *mut c_char,
    sep: *const c_char,
    p: *mut *mut c_char,
) -> *mut c_char {
    // SAFETY: caller-provided pointers; we mutate *p as our saveptr.
    unsafe {
        if s.is_null() {
            s = *p;
            if s.is_null() {
                return core::ptr::null_mut();
            }
        }
        // Skip leading separators.
        s = s.add(strspn(s, sep));
        if *s == 0 {
            *p = core::ptr::null_mut();
            return core::ptr::null_mut();
        }
        // Walk to the next separator and NUL-terminate the token there.
        let end = s.add(strcspn(s, sep));
        if *end != 0 {
            *end = 0;
            *p = end.add(1);
        } else {
            *p = core::ptr::null_mut();
        }
        s
    }
}

// strtok keeps its saveptr in module-private state. NOT thread-safe (matches
// upstream and the C standard).
//
// We use AtomicPtr<c_char> rather than `static mut` so we don't trip the
// Rust 2024 `static_mut_refs` lint; semantically it's still single-threaded
// state (the operations use Relaxed ordering).
static STRTOK_STATE: AtomicPtr<c_char> = AtomicPtr::new(core::ptr::null_mut());

/// `char *strtok(char *restrict s, const char *restrict sep)` — like
/// `strtok_r` but the saveptr is hidden in module state.
///
/// # Safety
/// See `strtok_r`. Caller must NOT call `strtok` from multiple threads on
/// overlapping searches.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strtok(s: *mut c_char, sep: *const c_char) -> *mut c_char {
    // SAFETY: forwarded to strtok_r against our private saveptr slot.
    unsafe {
        // Local saveptr, seeded from the static. Pass &mut local to strtok_r,
        // then write the updated value back into the static.
        let mut local = STRTOK_STATE.load(Ordering::Relaxed);
        let r = strtok_r(s, sep, core::ptr::addr_of_mut!(local));
        STRTOK_STATE.store(local, Ordering::Relaxed);
        r
    }
}

// ---------------------------------------------------------------------------
// strcasecmp / strncasecmp (+ locale wrappers)
// ---------------------------------------------------------------------------

/// `int strcasecmp(const char *l, const char *r)` — case-insensitive ASCII.
///
/// # Safety
/// Both pointers must be NUL-terminated.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strcasecmp(l: *const c_char, r: *const c_char) -> c_int {
    // SAFETY: caller guarantees both strings are NUL-terminated.
    unsafe {
        let mut l = l;
        let mut r = r;
        loop {
            let a = *l;
            let b = *r;
            if a == 0 || b == 0 {
                return fold(a) as c_int - fold(b) as c_int;
            }
            if a != b && fold(a) != fold(b) {
                return fold(a) as c_int - fold(b) as c_int;
            }
            l = l.add(1);
            r = r.add(1);
        }
    }
}

/// `int strncasecmp(const char *l, const char *r, size_t n)` — bounded
/// case-insensitive ASCII.
///
/// # Safety
/// Both pointers must be valid for `min(n, strlen+1)` bytes.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strncasecmp(l: *const c_char, r: *const c_char, mut n: size_t) -> c_int {
    if n == 0 {
        return 0;
    }
    // Upstream: `if (!n--) return 0;` then loop.
    n -= 1;
    // SAFETY: caller guarantees both buffers are valid for the bytes we touch.
    unsafe {
        let mut l = l;
        let mut r = r;
        while *l != 0 && *r != 0 && n != 0 && (*l == *r || fold(*l) == fold(*r)) {
            l = l.add(1);
            r = r.add(1);
            n -= 1;
        }
        fold(*l) as c_int - fold(*r) as c_int
    }
}

/// Internal name; upstream is `weak_alias(__strcasecmp_l, strcasecmp_l)`.
/// musl ignores the locale and forwards to `strcasecmp`; we do the same.
///
/// # Safety
/// See `strcasecmp`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn __strcasecmp_l(
    l: *const c_char,
    r: *const c_char,
    _loc: *mut c_void,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { strcasecmp(l, r) }
}

/// # Safety
/// See `strcasecmp`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strcasecmp_l(
    l: *const c_char,
    r: *const c_char,
    loc: *mut c_void,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { __strcasecmp_l(l, r, loc) }
}

/// Internal name; upstream is `weak_alias(__strncasecmp_l, strncasecmp_l)`.
/// musl ignores the locale and forwards to `strncasecmp`.
///
/// # Safety
/// See `strncasecmp`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn __strncasecmp_l(
    l: *const c_char,
    r: *const c_char,
    n: size_t,
    _loc: *mut c_void,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { strncasecmp(l, r, n) }
}

/// # Safety
/// See `strncasecmp`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn strncasecmp_l(
    l: *const c_char,
    r: *const c_char,
    n: size_t,
    loc: *mut c_void,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { __strncasecmp_l(l, r, n, loc) }
}

#[cfg(test)]
mod tests {
    use core::ffi::CStr;

    use super::*;

    fn cs(b: &[u8]) -> *const c_char {
        CStr::from_bytes_with_nul(b).unwrap().as_ptr() as *const c_char
    }

    fn cm(b: &mut [u8]) -> *mut c_char {
        // Caller passes a NUL-terminated slice; we hand back a writable ptr.
        assert_eq!(b.last(), Some(&0));
        b.as_mut_ptr() as *mut c_char
    }

    // ---- strstr -----------------------------------------------------

    #[test]
    fn strstr_basic() {
        let h = cs(b"hello world\0");
        let n = cs(b"world\0");
        // SAFETY: NUL-terminated literals.
        unsafe {
            let p = strstr(h, n);
            assert_eq!(p, h.add(6) as *mut c_char);
        }
    }

    #[test]
    fn strstr_empty_needle() {
        // SAFETY: NUL-terminated; per spec, empty needle matches at the start.
        unsafe {
            let h = cs(b"abc\0");
            assert_eq!(strstr(h, cs(b"\0")), h as *mut c_char);
        }
    }

    #[test]
    fn strstr_no_match() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert!(strstr(cs(b"hello\0"), cs(b"xyz\0")).is_null());
        }
    }

    #[test]
    fn strstr_overlap_then_match() {
        // The naive scan must rewind correctly: needle "aab" inside "aaab".
        let h = cs(b"aaab\0");
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strstr(h, cs(b"aab\0")), h.add(1) as *mut c_char);
        }
    }

    // ---- strspn / strcspn / strpbrk --------------------------------

    #[test]
    fn strspn_counts_prefix() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strspn(cs(b"abcdef\0"), cs(b"abc\0")), 3);
            assert_eq!(strspn(cs(b"aaab\0"), cs(b"a\0")), 3); // single-byte fast path
            assert_eq!(strspn(cs(b"xyz\0"), cs(b"abc\0")), 0);
            assert_eq!(strspn(cs(b"abc\0"), cs(b"\0")), 0); // empty c
        }
    }

    #[test]
    fn strcspn_counts_prefix_until_match() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strcspn(cs(b"hello,world\0"), cs(b",\0")), 5);
            assert_eq!(strcspn(cs(b"hello\0"), cs(b"xyz\0")), 5);
            assert_eq!(strcspn(cs(b"abc\0"), cs(b"\0")), 3); // empty c → strlen(s)
                                                             // Multi-byte c uses the byteset path.
            assert_eq!(strcspn(cs(b"abc;def\0"), cs(b"x;\0")), 3);
        }
    }

    #[test]
    fn strpbrk_finds_first_or_null() {
        let s = cs(b"hello,world!\0");
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strpbrk(s, cs(b",!\0")), s.add(5) as *mut c_char);
            assert!(strpbrk(s, cs(b"xyz\0")).is_null());
        }
    }

    // ---- strsep ----------------------------------------------------

    #[test]
    fn strsep_walks_tokens_with_empty_runs() {
        // strsep treats consecutive separators as empty tokens.
        let mut buf: [u8; 16] = *b"a,,b,c\0\0\0\0\0\0\0\0\0\0";
        let sep = cs(b",\0");
        // SAFETY: writable NUL-terminated buffer.
        unsafe {
            let mut state: *mut c_char = cm(&mut buf);
            let p = core::ptr::addr_of_mut!(state);

            let t0 = strsep(p, sep);
            assert_eq!(CStr::from_ptr(t0 as *const _).to_bytes(), b"a");
            let t1 = strsep(p, sep);
            assert_eq!(CStr::from_ptr(t1 as *const _).to_bytes(), b""); // empty
            let t2 = strsep(p, sep);
            assert_eq!(CStr::from_ptr(t2 as *const _).to_bytes(), b"b");
            let t3 = strsep(p, sep);
            assert_eq!(CStr::from_ptr(t3 as *const _).to_bytes(), b"c");
            let t4 = strsep(p, sep);
            assert!(t4.is_null());
        }
    }

    // ---- strtok / strtok_r ----------------------------------------

    #[test]
    fn strtok_r_skips_runs_of_separators() {
        // strtok (and _r) collapses runs of separators (unlike strsep).
        let mut buf: [u8; 16] = *b",,a,,b,,c,,\0\0\0\0\0";
        let sep = cs(b",\0");
        // SAFETY: writable NUL-terminated buffer.
        unsafe {
            let mut state: *mut c_char = core::ptr::null_mut();
            let p = core::ptr::addr_of_mut!(state);

            let t0 = strtok_r(cm(&mut buf), sep, p);
            assert_eq!(CStr::from_ptr(t0 as *const _).to_bytes(), b"a");
            let t1 = strtok_r(core::ptr::null_mut(), sep, p);
            assert_eq!(CStr::from_ptr(t1 as *const _).to_bytes(), b"b");
            let t2 = strtok_r(core::ptr::null_mut(), sep, p);
            assert_eq!(CStr::from_ptr(t2 as *const _).to_bytes(), b"c");
            let t3 = strtok_r(core::ptr::null_mut(), sep, p);
            assert!(t3.is_null());
        }
    }

    #[test]
    fn strtok_uses_module_state() {
        let mut buf: [u8; 16] = *b"x:y:z\0\0\0\0\0\0\0\0\0\0\0";
        let sep = cs(b":\0");
        // SAFETY: writable NUL-terminated buffer; not thread-safe but this
        // test runs in isolation.
        unsafe {
            let t0 = strtok(cm(&mut buf), sep);
            assert_eq!(CStr::from_ptr(t0 as *const _).to_bytes(), b"x");
            let t1 = strtok(core::ptr::null_mut(), sep);
            assert_eq!(CStr::from_ptr(t1 as *const _).to_bytes(), b"y");
            let t2 = strtok(core::ptr::null_mut(), sep);
            assert_eq!(CStr::from_ptr(t2 as *const _).to_bytes(), b"z");
            let t3 = strtok(core::ptr::null_mut(), sep);
            assert!(t3.is_null());
        }
    }

    // ---- strcasecmp / strncasecmp ---------------------------------

    #[test]
    fn strcasecmp_orders_case_insensitive() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strcasecmp(cs(b"Hello\0"), cs(b"hello\0")), 0);
            assert_eq!(strcasecmp(cs(b"abc\0"), cs(b"ABC\0")), 0);
            assert!(strcasecmp(cs(b"abc\0"), cs(b"abd\0")) < 0);
            assert!(strcasecmp(cs(b"abd\0"), cs(b"abc\0")) > 0);
            // Length difference.
            assert!(strcasecmp(cs(b"ab\0"), cs(b"abc\0")) < 0);
        }
    }

    #[test]
    fn strncasecmp_respects_n_and_case() {
        // SAFETY: NUL-terminated literals.
        unsafe {
            assert_eq!(strncasecmp(cs(b"Hello\0"), cs(b"hELLo\0"), 5), 0);
            // Case-equivalent first 3 bytes; differ at byte 3.
            assert!(strncasecmp(cs(b"Abcdef\0"), cs(b"aBcXyz\0"), 4) < 0);
            assert_eq!(strncasecmp(cs(b"a\0"), cs(b"b\0"), 0), 0);
        }
    }

    #[test]
    fn case_locale_wrappers_forward() {
        // SAFETY: NUL-terminated literals; loc is unused.
        unsafe {
            assert_eq!(
                __strcasecmp_l(cs(b"FOO\0"), cs(b"foo\0"), core::ptr::null_mut()),
                0
            );
            assert_eq!(
                strncasecmp_l(cs(b"BARx\0"), cs(b"barY\0"), 3, core::ptr::null_mut()),
                0
            );
        }
    }
}
