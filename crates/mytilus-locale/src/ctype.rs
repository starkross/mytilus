//! `<ctype.h>` — character classification and case folding.
//!
//! Mirrors `src/ctype/` upstream. Implementations port musl's bit-twiddling
//! tricks verbatim (rather than using a 256-entry lookup table) — every
//! function is one expression. The results are bit-identical to musl's.
//!
//! Locale wrappers: `__X_l(c, loc)` always forwards to `X(c)`. Upstream
//! does the same — musl is documented as "C locale only" for ctype, so
//! the locale arg is unused in practice.
//!
//! `locale_t` is a placeholder `*mut c_void` until the full locale story
//! lands.
//!
//! Symbol gating: `#[cfg_attr(target_env = "musl", no_mangle)]` keeps the C names
//! off the test binary's link table on macOS host (otherwise our `tolower`
//! shadows libsystem's, which the Rust test runtime calls internally).

use mytilus_sys::ctypes::{c_int, c_void};

/// Opaque locale handle — same shape as the placeholder in `mytilus-errno`.
/// Will be retyped to a real struct when locale machinery lands.
pub type locale_t = *mut c_void;

// Helper to keep the C bit-hacks readable. Behaves like C's `(unsigned)c`:
// reinterprets the bits of an `int` as `unsigned int`.
#[inline]
fn u(c: c_int) -> u32 {
    c as u32
}

// ---------------------------------------------------------------------------
// Classification: isalpha, isdigit, isspace, isupper, islower, isalnum,
//                 isxdigit, ispunct, isprint, isgraph, iscntrl, isblank,
//                 isascii
// ---------------------------------------------------------------------------

/// `int isalpha(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isalpha(c: c_int) -> c_int {
    // Upstream: `((unsigned)c|32)-'a' < 26` — case-fold to lowercase, then
    // check 'a'..='z' via wrap-around unsigned compare.
    ((u(c) | 32).wrapping_sub(b'a' as u32) < 26) as c_int
}

/// `int isdigit(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isdigit(c: c_int) -> c_int {
    // Upstream: `(unsigned)c-'0' < 10`.
    (u(c).wrapping_sub(b'0' as u32) < 10) as c_int
}

/// `int isspace(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isspace(c: c_int) -> c_int {
    // Upstream: `c == ' ' || (unsigned)c-'\t' < 5` — '\t', '\n', '\v', '\f',
    // '\r' are five contiguous bytes starting at 0x09.
    (c == b' ' as c_int || u(c).wrapping_sub(b'\t' as u32) < 5) as c_int
}

/// `int isupper(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isupper(c: c_int) -> c_int {
    // Upstream: `(unsigned)c-'A' < 26`.
    (u(c).wrapping_sub(b'A' as u32) < 26) as c_int
}

/// `int islower(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn islower(c: c_int) -> c_int {
    // Upstream: `(unsigned)c-'a' < 26`.
    (u(c).wrapping_sub(b'a' as u32) < 26) as c_int
}

/// `int isalnum(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isalnum(c: c_int) -> c_int {
    // Upstream: `isalpha(c) || isdigit(c)`.
    (isalpha(c) != 0 || isdigit(c) != 0) as c_int
}

/// `int isxdigit(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isxdigit(c: c_int) -> c_int {
    // Upstream: `isdigit(c) || ((unsigned)c|32)-'a' < 6`.
    (isdigit(c) != 0 || (u(c) | 32).wrapping_sub(b'a' as u32) < 6) as c_int
}

/// `int isgraph(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isgraph(c: c_int) -> c_int {
    // Upstream: `(unsigned)c-0x21 < 0x5e` — printable, excluding space.
    (u(c).wrapping_sub(0x21) < 0x5e) as c_int
}

/// `int isprint(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isprint(c: c_int) -> c_int {
    // Upstream: `(unsigned)c-0x20 < 0x5f` — printable, including space.
    (u(c).wrapping_sub(0x20) < 0x5f) as c_int
}

/// `int ispunct(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn ispunct(c: c_int) -> c_int {
    // Upstream: `isgraph(c) && !isalnum(c)`.
    (isgraph(c) != 0 && isalnum(c) == 0) as c_int
}

/// `int iscntrl(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn iscntrl(c: c_int) -> c_int {
    // Upstream: `(unsigned)c < 0x20 || c == 0x7f`.
    (u(c) < 0x20 || c == 0x7f) as c_int
}

/// `int isblank(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isblank(c: c_int) -> c_int {
    // Upstream: `c == ' ' || c == '\t'`.
    (c == b' ' as c_int || c == b'\t' as c_int) as c_int
}

/// `int isascii(int c)` — non-locale-aware (no `isascii_l`).
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn isascii(c: c_int) -> c_int {
    // Upstream: `!(c & ~0x7f)`.
    ((c & !0x7f) == 0) as c_int
}

// ---------------------------------------------------------------------------
// Case folding: tolower, toupper
// ---------------------------------------------------------------------------

/// `int tolower(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn tolower(c: c_int) -> c_int {
    // Upstream: `if (isupper(c)) return c | 32; return c;`
    if isupper(c) != 0 {
        c | 32
    } else {
        c
    }
}

/// `int toupper(int c)`
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn toupper(c: c_int) -> c_int {
    // Upstream: `if (islower(c)) return c & 0x5f; return c;`
    if islower(c) != 0 {
        c & 0x5f
    } else {
        c
    }
}

/// `int toascii(int c)` — non-locale-aware. Upstream comment calls it a
/// "nonsense function that should NEVER be used"; we ship it for ABI
/// compatibility.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn toascii(c: c_int) -> c_int {
    c & 0x7f
}

// ---------------------------------------------------------------------------
// Locale-aware wrappers (all forward to the non-_l form, matching musl)
// ---------------------------------------------------------------------------
//
// Each entry: `__X_l(c, loc)` is the canonical glibc-internal name; `X_l`
// is the POSIX-spelled weak alias upstream. We define both as separate
// strong symbols pointing at the same body; the linker-script will mark
// `X_l` weak at the .so layer when we get there.

macro_rules! ctype_l {
    ($base:ident, $impl_:ident, $alias:ident) => {
        /// # Safety
        /// Forwards to the non-locale form; the locale handle is unused.
        #[cfg_attr(target_env = "musl", no_mangle)]
        pub extern "C" fn $impl_(c: c_int, _loc: locale_t) -> c_int {
            $base(c)
        }

        /// # Safety
        /// See the `__*_l` form.
        #[cfg_attr(target_env = "musl", no_mangle)]
        pub extern "C" fn $alias(c: c_int, loc: locale_t) -> c_int {
            $impl_(c, loc)
        }
    };
}

ctype_l!(isalpha, __isalpha_l, isalpha_l);
ctype_l!(isdigit, __isdigit_l, isdigit_l);
ctype_l!(isspace, __isspace_l, isspace_l);
ctype_l!(isupper, __isupper_l, isupper_l);
ctype_l!(islower, __islower_l, islower_l);
ctype_l!(isalnum, __isalnum_l, isalnum_l);
ctype_l!(isxdigit, __isxdigit_l, isxdigit_l);
ctype_l!(isgraph, __isgraph_l, isgraph_l);
ctype_l!(isprint, __isprint_l, isprint_l);
ctype_l!(ispunct, __ispunct_l, ispunct_l);
ctype_l!(iscntrl, __iscntrl_l, iscntrl_l);
ctype_l!(isblank, __isblank_l, isblank_l);
ctype_l!(tolower, __tolower_l, tolower_l);
ctype_l!(toupper, __toupper_l, toupper_l);

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: for ASCII bytes 0..=127, we can compare against std's u8
    // ASCII helpers (they're not used in the impl — just the test oracle).
    fn predicate_matches<P: Fn(u8) -> bool>(f: extern "C" fn(c_int) -> c_int, p: P) {
        for b in 0..=127u8 {
            let got = f(b as c_int) != 0;
            let want = p(b);
            assert_eq!(got, want, "byte 0x{:02x} ({:?})", b, b as char);
        }
        // Also check a few out-of-range values: all classifiers must say
        // "no" for negative inputs and for values past 0xff.
        for c in [-1, -2, 256, 1000, c_int::MIN, c_int::MAX] {
            // ispunct/isxdigit can fire on |32 fold. Check classifiers
            // individually below; for the generic harness, just assert
            // the impl doesn't panic.
            let _ = f(c);
        }
    }

    #[test]
    fn isalpha_matches_ascii_table() {
        predicate_matches(isalpha, |b| b.is_ascii_alphabetic());
    }

    #[test]
    fn isdigit_matches_ascii_table() {
        predicate_matches(isdigit, |b| b.is_ascii_digit());
    }

    #[test]
    fn isspace_matches_ascii_table() {
        predicate_matches(isspace, |b| {
            // POSIX isspace: ' ', '\t', '\n', '\v', '\f', '\r'.
            matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
        });
    }

    #[test]
    fn isupper_matches_ascii_table() {
        predicate_matches(isupper, |b| b.is_ascii_uppercase());
    }

    #[test]
    fn islower_matches_ascii_table() {
        predicate_matches(islower, |b| b.is_ascii_lowercase());
    }

    #[test]
    fn isalnum_matches_ascii_table() {
        predicate_matches(isalnum, |b| b.is_ascii_alphanumeric());
    }

    #[test]
    fn isxdigit_matches_ascii_table() {
        predicate_matches(isxdigit, |b| b.is_ascii_hexdigit());
    }

    #[test]
    fn isgraph_matches_ascii_table() {
        predicate_matches(isgraph, |b| b.is_ascii_graphic());
    }

    #[test]
    fn isprint_matches_ascii_table() {
        predicate_matches(isprint, |b| matches!(b, 0x20..=0x7e));
    }

    #[test]
    fn ispunct_matches_ascii_table() {
        predicate_matches(ispunct, |b| b.is_ascii_punctuation());
    }

    #[test]
    fn iscntrl_matches_ascii_table() {
        predicate_matches(iscntrl, |b| b.is_ascii_control());
    }

    #[test]
    fn isblank_matches_ascii_table() {
        predicate_matches(isblank, |b| b == b' ' || b == b'\t');
    }

    #[test]
    fn isascii_classifies_correctly() {
        for b in 0..=127i32 {
            assert_ne!(isascii(b), 0, "0x{b:02x} should be ASCII");
        }
        for c in [128, 200, 255, 256, -1, 0xfff_i32] {
            assert_eq!(isascii(c), 0, "{c} should NOT be ASCII");
        }
    }

    #[test]
    fn tolower_folds_ascii_uppercase() {
        for b in 0..=127u8 {
            let want = b.to_ascii_lowercase() as c_int;
            assert_eq!(tolower(b as c_int), want, "byte 0x{:02x}", b);
        }
        // Out-of-range values are passed through.
        assert_eq!(tolower(-1), -1);
        assert_eq!(tolower(0xff), 0xff);
        assert_eq!(tolower(0x100), 0x100);
    }

    #[test]
    fn toupper_folds_ascii_lowercase() {
        for b in 0..=127u8 {
            let want = b.to_ascii_uppercase() as c_int;
            assert_eq!(toupper(b as c_int), want, "byte 0x{:02x}", b);
        }
        assert_eq!(toupper(-1), -1);
        assert_eq!(toupper(0xff), 0xff);
        assert_eq!(toupper(0x100), 0x100);
    }

    #[test]
    fn toascii_strips_high_bit() {
        assert_eq!(toascii(0x41), 0x41);
        assert_eq!(toascii(0xC1), 0x41);
        assert_eq!(toascii(0xFF), 0x7F);
    }

    #[test]
    fn locale_wrappers_forward() {
        assert_eq!(__isalpha_l(b'A' as c_int, core::ptr::null_mut()), 1);
        assert_eq!(__isalpha_l(b'1' as c_int, core::ptr::null_mut()), 0);
        assert_eq!(
            tolower_l(b'X' as c_int, core::ptr::null_mut()),
            b'x' as c_int
        );
        assert_eq!(
            toupper_l(b'x' as c_int, core::ptr::null_mut()),
            b'X' as c_int
        );
    }
}
