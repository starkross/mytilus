//! AArch64 Linux syscall numbers.
//!
//! Authoritative source: `arch/aarch64/bits/syscall.h.in` upstream, which
//! mirrors `<linux/asm-generic/unistd.h>`. These values are kernel ABI and
//! never change.
//!
//! Crates add entries here as they grow. We populate lazily rather than
//! pre-defining the full ~300-entry table because most consumers only need
//! a handful and reviewing one syscall at a time keeps the diff readable.

#![allow(dead_code)]

use crate::ctypes::c_long;

// ---------------------------------------------------------------------------
// memory management (consumed by mytilus-mman)
// ---------------------------------------------------------------------------

pub const SYS_munmap: c_long = 215;
pub const SYS_mremap: c_long = 216;
pub const SYS_mmap: c_long = 222;
pub const SYS_mprotect: c_long = 226;
pub const SYS_msync: c_long = 227;
pub const SYS_mlock: c_long = 228;
pub const SYS_munlock: c_long = 229;
pub const SYS_mlockall: c_long = 230;
pub const SYS_munlockall: c_long = 231;
pub const SYS_mincore: c_long = 232;
pub const SYS_madvise: c_long = 233;
