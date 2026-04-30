//! `mytilus-stdlib` — `<stdlib.h>` integer helpers, sort, search, env, exit.
//!
//! Phase 1 ports:
//! - `abs`/`labs`/`llabs`/`imaxabs` (absolute value)
//! - `div`/`ldiv`/`lldiv`/`imaxdiv` (division yielding `{quot, rem}`)
//! - `qsort` / `qsort_r` (sort by callback comparator)
//! - `bsearch` (binary search by callback comparator)
//! - `div_t`/`ldiv_t`/`lldiv_t`/`imaxdiv_t` (returned by value from div family)
//!
//! Deferred to later phases (need malloc, env, file I/O, or string parsing):
//! `strtol`/`strtoll`/`strtoul`/`strtoull`/`strtoimax`/`strtoumax`,
//! `atoi`/`atol`/`atoll`, `atof`/`strtod`, `getenv`/`setenv`/`putenv`/
//! `unsetenv`, `exit`/`atexit`/`_Exit`, `mblen`/`mbtowc`/`wctomb`/`mbstowcs`/
//! `wcstombs`, `system`, `realpath`, `mkstemp`/`mkdtemp`.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

mod int_ops;
mod sort;

pub use int_ops::*;
pub use sort::*;
