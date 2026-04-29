//! `mytilus-locale` — locale, iconv, multibyte, message catalog.
//!
//! Phase 1 ports just the **ctype** subset from `src/ctype/` upstream
//! (`isalpha`, `isdigit`, `isspace`, `isupper`, `islower`, `isalnum`,
//! `isxdigit`, `ispunct`, `isprint`, `isgraph`, `iscntrl`, `isblank`,
//! `isascii`, `tolower`, `toupper`, `toascii`, plus the `__X_l` /
//! `X_l` locale-aware wrappers that all forward to the C locale).
//!
//! Why ctype lives here: in our workspace plan ctype shares a crate with
//! locale because the `_l` variants take a `locale_t` and "ctype + locale"
//! is the smallest natural unit. The functions themselves don't actually
//! consult the locale — musl is documented as "C locale only" for ctype, so
//! `__isalpha_l(c, loc)` is just `isalpha(c)`. We mirror that.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod ctype;

pub use ctype::*;
