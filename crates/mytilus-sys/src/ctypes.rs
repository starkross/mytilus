//! C scalar typedefs for aarch64-linux 64-bit.
//!
//! These match what the upstream headers (`bits/alltypes.h.in` for aarch64)
//! produce. Because we target a single ABI, every type below is concrete —
//! no `cfg(target_pointer_width = …)` branching.

#![allow(clippy::upper_case_acronyms)]

pub type c_char = u8; // AArch64 PCS: `char` is unsigned.
pub type c_schar = i8;
pub type c_uchar = u8;

pub type c_short = i16;
pub type c_ushort = u16;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64; // LP64.
pub type c_ulong = u64;
pub type c_longlong = i64;
pub type c_ulonglong = u64;

pub type c_float = f32;
pub type c_double = f64;
// `long double` on aarch64 Linux is IEEE binary128 (128-bit). Rust has no
// stable f128; we model it as `[u64; 2]` at the FFI boundary and use the
// `compiler-rt`-provided softfloat helpers (or hardware where available).
#[repr(C, align(16))]
#[derive(Copy, Clone)]
pub struct c_longdouble(pub [u64; 2]);

pub type c_void = core::ffi::c_void;

// POSIX-mandated typedefs. 64-bit only — no time32, no off32.
pub type size_t = usize;
pub type ssize_t = isize;
pub type ptrdiff_t = isize;
pub type intptr_t = isize;
pub type uintptr_t = usize;

pub type off_t = i64;
pub type loff_t = i64;
pub type time_t = i64;
pub type suseconds_t = i64;
pub type clock_t = i64;
pub type clockid_t = i32;
pub type pid_t = i32;
pub type uid_t = u32;
pub type gid_t = u32;
pub type mode_t = u32;
pub type dev_t = u64;
pub type ino_t = u64;
pub type nlink_t = u32;
pub type blksize_t = i64;
pub type blkcnt_t = i64;
pub type id_t = u32;
pub type key_t = i32;

pub type socklen_t = u32;
pub type sa_family_t = u16;
