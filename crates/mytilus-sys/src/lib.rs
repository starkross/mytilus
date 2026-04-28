//! `mytilus-sys` — raw aarch64 Linux syscall layer and C type definitions.
//!
//! This is the only crate in the workspace allowed to contain `svc #0`.
//! Everything else routes through these wrappers.
//!
//! Target: aarch64-unknown-linux, 64-bit only.
//!
//! Status: skeleton.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod ctypes;
pub mod errno_raw;
pub mod syscall;

pub use ctypes::*;
