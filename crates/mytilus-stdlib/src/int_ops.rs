//! Integer helpers: `abs` family and `div` family.
//!
//! Mirrors `src/stdlib/{abs,labs,llabs,imaxabs,div,ldiv,lldiv,imaxdiv}.c`
//! upstream. All trivial — `abs(x) = x>0 ? x : -x`, `div(num,den) = {num/den, num%den}`.
//!
//! On `INT_MIN` the upstream `-a` overflow is implementation-defined behavior
//! in C and works out to `INT_MIN` in practice on 2's-complement machines.
//! Rust's `-a` panics in debug, so we use `wrapping_neg()` to match the
//! observable upstream behavior.
//!
//! `intmax_t`: POSIX requires it be the widest integer type. On LP64 +
//! `long long = 64-bit`, that's `i64`. We define it locally since
//! `mytilus-sys::ctypes` doesn't export an alias for it (yet).

use mytilus_sys::ctypes::{c_int, c_long, c_longlong};

/// `intmax_t` on LP64 = `i64`. Locally aliased; promote to `mytilus-sys`
/// when another consumer needs it.
type intmax_t = i64;

// ---------------------------------------------------------------------------
// abs family
// ---------------------------------------------------------------------------

/// `int abs(int a)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn abs(a: c_int) -> c_int {
    if a > 0 {
        a
    } else {
        a.wrapping_neg()
    }
}

/// `long labs(long a)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn labs(a: c_long) -> c_long {
    if a > 0 {
        a
    } else {
        a.wrapping_neg()
    }
}

/// `long long llabs(long long a)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn llabs(a: c_longlong) -> c_longlong {
    if a > 0 {
        a
    } else {
        a.wrapping_neg()
    }
}

/// `intmax_t imaxabs(intmax_t a)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn imaxabs(a: intmax_t) -> intmax_t {
    if a > 0 {
        a
    } else {
        a.wrapping_neg()
    }
}

// ---------------------------------------------------------------------------
// div family
// ---------------------------------------------------------------------------
//
// All four structs are `{ quot, rem }` of the corresponding integer width.
// Returned by value across the C ABI.

/// `div_t` — `{ int quot; int rem; }`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct div_t {
    pub quot: c_int,
    pub rem: c_int,
}

/// `ldiv_t` — `{ long quot; long rem; }`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ldiv_t {
    pub quot: c_long,
    pub rem: c_long,
}

/// `lldiv_t` — `{ long long quot; long long rem; }`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct lldiv_t {
    pub quot: c_longlong,
    pub rem: c_longlong,
}

/// `imaxdiv_t` — `{ intmax_t quot; intmax_t rem; }`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct imaxdiv_t {
    pub quot: intmax_t,
    pub rem: intmax_t,
}

/// `div_t div(int num, int den)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn div(num: c_int, den: c_int) -> div_t {
    div_t {
        quot: num / den,
        rem: num % den,
    }
}

/// `ldiv_t ldiv(long num, long den)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn ldiv(num: c_long, den: c_long) -> ldiv_t {
    ldiv_t {
        quot: num / den,
        rem: num % den,
    }
}

/// `lldiv_t lldiv(long long num, long long den)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn lldiv(num: c_longlong, den: c_longlong) -> lldiv_t {
    lldiv_t {
        quot: num / den,
        rem: num % den,
    }
}

/// `imaxdiv_t imaxdiv(intmax_t num, intmax_t den)`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn imaxdiv(num: intmax_t, den: intmax_t) -> imaxdiv_t {
    imaxdiv_t {
        quot: num / den,
        rem: num % den,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- abs family ---------------------------------------------------

    #[test]
    fn abs_basic() {
        assert_eq!(abs(0), 0);
        assert_eq!(abs(1), 1);
        assert_eq!(abs(-1), 1);
        assert_eq!(abs(c_int::MAX), c_int::MAX);
        // INT_MIN: upstream behavior is INT_MIN (overflow wraps).
        assert_eq!(abs(c_int::MIN), c_int::MIN);
    }

    #[test]
    fn labs_basic() {
        assert_eq!(labs(0), 0);
        assert_eq!(labs(-42), 42);
        assert_eq!(labs(c_long::MIN), c_long::MIN);
    }

    #[test]
    fn llabs_basic() {
        assert_eq!(llabs(0), 0);
        assert_eq!(llabs(-c_longlong::MAX), c_longlong::MAX);
        assert_eq!(llabs(c_longlong::MIN), c_longlong::MIN);
    }

    #[test]
    fn imaxabs_basic() {
        assert_eq!(imaxabs(0), 0);
        assert_eq!(imaxabs(-7), 7);
        assert_eq!(imaxabs(intmax_t::MIN), intmax_t::MIN);
    }

    // ---- div family ---------------------------------------------------

    #[test]
    fn div_basic() {
        assert_eq!(div(7, 2), div_t { quot: 3, rem: 1 });
        // Negative dividend: C99 says truncation toward zero.
        assert_eq!(div(-7, 2), div_t { quot: -3, rem: -1 });
        assert_eq!(div(7, -2), div_t { quot: -3, rem: 1 });
        assert_eq!(div(0, 5), div_t { quot: 0, rem: 0 });
    }

    #[test]
    fn ldiv_basic() {
        assert_eq!(
            ldiv(1_000_000_000_000, 7),
            ldiv_t {
                quot: 142_857_142_857,
                rem: 1
            }
        );
    }

    #[test]
    fn lldiv_basic() {
        assert_eq!(lldiv(100, 9), lldiv_t { quot: 11, rem: 1 });
    }

    #[test]
    fn imaxdiv_basic() {
        assert_eq!(imaxdiv(7, -3), imaxdiv_t { quot: -2, rem: 1 });
    }

    // ---- struct layout (C ABI) ----------------------------------------

    #[test]
    fn div_struct_layouts() {
        use core::mem::{align_of, offset_of, size_of};
        // div_t: 8 bytes (two i32s), 4-byte aligned.
        assert_eq!(size_of::<div_t>(), 8);
        assert_eq!(align_of::<div_t>(), 4);
        assert_eq!(offset_of!(div_t, quot), 0);
        assert_eq!(offset_of!(div_t, rem), 4);
        // ldiv_t: 16 bytes (two i64s), 8-byte aligned.
        assert_eq!(size_of::<ldiv_t>(), 16);
        assert_eq!(align_of::<ldiv_t>(), 8);
        assert_eq!(offset_of!(ldiv_t, quot), 0);
        assert_eq!(offset_of!(ldiv_t, rem), 8);
        // lldiv_t and imaxdiv_t are the same shape as ldiv_t on LP64.
        assert_eq!(size_of::<lldiv_t>(), 16);
        assert_eq!(size_of::<imaxdiv_t>(), 16);
    }
}
