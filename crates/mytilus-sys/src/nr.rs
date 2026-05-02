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

// ---------------------------------------------------------------------------
// time (consumed by mytilus-time)
// ---------------------------------------------------------------------------

pub const SYS_nanosleep: c_long = 101;
pub const SYS_clock_settime: c_long = 112;
pub const SYS_clock_gettime: c_long = 113;
pub const SYS_clock_getres: c_long = 114;
pub const SYS_clock_nanosleep: c_long = 115;
pub const SYS_gettimeofday: c_long = 169;

// ---------------------------------------------------------------------------
// fcntl / fd ops (consumed by mytilus-fcntl)
// ---------------------------------------------------------------------------
//
// Note: aarch64 has NO `SYS_open` — only `SYS_openat`. Upstream's `open()`
// on aarch64 routes through `openat(AT_FDCWD, …)`. We do the same.

pub const SYS_fcntl: c_long = 25;
pub const SYS_fallocate: c_long = 47;
pub const SYS_openat: c_long = 56;
pub const SYS_close: c_long = 57;
pub const SYS_fadvise64: c_long = 223;

// ---------------------------------------------------------------------------
// fd shuffle + sync (consumed by mytilus-unistd)
// ---------------------------------------------------------------------------
//
// Note: aarch64 has no `SYS_dup2` (use `SYS_dup3` with flags=0) and no
// `SYS_pause` (use `SYS_ppoll(0,0,0,0)` — waits forever on no fds).

pub const SYS_dup: c_long = 23;
pub const SYS_dup3: c_long = 24;
pub const SYS_lseek: c_long = 62;
pub const SYS_read: c_long = 63;
pub const SYS_write: c_long = 64;
pub const SYS_pread64: c_long = 67;
pub const SYS_pwrite64: c_long = 68;
pub const SYS_ppoll: c_long = 73;
pub const SYS_sync: c_long = 81;
pub const SYS_fsync: c_long = 82;
pub const SYS_fdatasync: c_long = 83;

// ---------------------------------------------------------------------------
// process / pid / uid / gid / signal-delivery (consumed by mytilus-process)
// ---------------------------------------------------------------------------

pub const SYS_exit: c_long = 93;
pub const SYS_exit_group: c_long = 94;
pub const SYS_sched_yield: c_long = 124;
pub const SYS_kill: c_long = 129;
pub const SYS_setpgid: c_long = 154;
pub const SYS_getpgid: c_long = 155;
pub const SYS_getsid: c_long = 156;
pub const SYS_setsid: c_long = 157;
pub const SYS_getpid: c_long = 172;
pub const SYS_getppid: c_long = 173;
pub const SYS_getuid: c_long = 174;
pub const SYS_geteuid: c_long = 175;
pub const SYS_getgid: c_long = 176;
pub const SYS_getegid: c_long = 177;
