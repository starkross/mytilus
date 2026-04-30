//! `mytilus-fcntl` — `<fcntl.h>`: `open`, `openat`, `creat`, `fcntl`,
//! `posix_fadvise`, `posix_fallocate`.
//!
//! Phase 1 ports the six public functions from `src/fcntl/` upstream. Three
//! of them (`open`, `openat`, `fcntl`) are **variadic in C**: the third arg
//! is consumed only when a flag/cmd indicates it's present (`O_CREAT`/
//! `O_TMPFILE` for the open family; per-cmd for `fcntl`). We use Rust's
//! unstable `c_variadic` feature (stable in nightly) to expose the same
//! shape.
//!
//! AArch64 specialization: the kernel has no `SYS_open` — only `SYS_openat`.
//! `open(path, flags, mode)` routes through `openat(AT_FDCWD, …)`. Upstream
//! does the same via `__sys_open_cp`.
//!
//! TODO(thread/cancel): upstream wraps the kernel call in `__syscall_cp`
//! (cancellation point) for `open`/`openat`/`fcntl(F_SETLKW)`. We use plain
//! `svc`. Switch when `mytilus-thread`'s asm lands.
//!
//! TODO(compat): we drop two upstream workarounds for old kernels. Both
//! TODOs sit inside `fcntl` body:
//! - `F_GETOWN` upstream fakes via `F_GETOWN_EX` to disambiguate
//!   process-group returns from errors (kernel returns negative for both).
//!   We pass `F_GETOWN` directly — fine on modern aarch64 kernels, but
//!   negative return is ambiguous.
//! - `F_DUPFD_CLOEXEC` upstream falls back to `F_DUPFD + F_SETFD` on
//!   kernels lacking the cloexec variant. We pass through. Modern
//!   aarch64 kernels (≥2.6.24) always support it.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![feature(c_variadic)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

extern crate mytilus_errno;

use mytilus_sys::ctypes::{c_char, c_int, c_long, c_ulong, mode_t, off_t};
use mytilus_sys::nr::*;
use mytilus_sys::syscall::{ret, syscall3, syscall4};

// ---------------------------------------------------------------------------
// Constants — kernel ABI for AArch64 Linux. Values match `<bits/fcntl.h>`
// upstream and `<linux/fcntl.h>`.
// ---------------------------------------------------------------------------

// Access modes
pub const O_RDONLY: c_int = 0o0;
pub const O_WRONLY: c_int = 0o1;
pub const O_RDWR: c_int = 0o2;

// Status flags
pub const O_CREAT: c_int = 0o100;
pub const O_EXCL: c_int = 0o200;
pub const O_NOCTTY: c_int = 0o400;
pub const O_TRUNC: c_int = 0o1000;
pub const O_APPEND: c_int = 0o2000;
pub const O_NONBLOCK: c_int = 0o4000;
pub const O_NDELAY: c_int = O_NONBLOCK;
pub const O_DSYNC: c_int = 0o10000;
pub const O_SYNC: c_int = 0o4010000;
pub const O_RSYNC: c_int = 0o4010000;
pub const O_DIRECTORY: c_int = 0o40000;
pub const O_NOFOLLOW: c_int = 0o100000;
pub const O_CLOEXEC: c_int = 0o2000000;
pub const O_ASYNC: c_int = 0o20000;
pub const O_DIRECT: c_int = 0o200000;
pub const O_LARGEFILE: c_int = 0o400000;
pub const O_NOATIME: c_int = 0o1000000;
pub const O_PATH: c_int = 0o10000000;
pub const O_TMPFILE: c_int = 0o20040000;
pub const O_SEARCH: c_int = O_PATH;
pub const O_EXEC: c_int = O_PATH;
pub const O_ACCMODE: c_int = 0o3 | O_SEARCH;

// fcntl commands
pub const F_DUPFD: c_int = 0;
pub const F_GETFD: c_int = 1;
pub const F_SETFD: c_int = 2;
pub const F_GETFL: c_int = 3;
pub const F_SETFL: c_int = 4;
pub const F_GETLK: c_int = 5;
pub const F_SETLK: c_int = 6;
pub const F_SETLKW: c_int = 7;
pub const F_SETOWN: c_int = 8;
pub const F_GETOWN: c_int = 9;
pub const F_SETSIG: c_int = 10;
pub const F_GETSIG: c_int = 11;
pub const F_SETOWN_EX: c_int = 15;
pub const F_GETOWN_EX: c_int = 16;
pub const F_DUPFD_CLOEXEC: c_int = 1030;

// File-lock types
pub const F_RDLCK: c_int = 0;
pub const F_WRLCK: c_int = 1;
pub const F_UNLCK: c_int = 2;

// FD flags
pub const FD_CLOEXEC: c_int = 1;

// AT_* (path-resolution flags shared with `*at` family)
pub const AT_FDCWD: c_int = -100;
pub const AT_SYMLINK_NOFOLLOW: c_int = 0x100;
pub const AT_REMOVEDIR: c_int = 0x200;
pub const AT_SYMLINK_FOLLOW: c_int = 0x400;
pub const AT_EACCESS: c_int = 0x200;
pub const AT_NO_AUTOMOUNT: c_int = 0x800;
pub const AT_EMPTY_PATH: c_int = 0x1000;

// posix_fadvise advice
pub const POSIX_FADV_NORMAL: c_int = 0;
pub const POSIX_FADV_RANDOM: c_int = 1;
pub const POSIX_FADV_SEQUENTIAL: c_int = 2;
pub const POSIX_FADV_WILLNEED: c_int = 3;
pub const POSIX_FADV_DONTNEED: c_int = 4;
pub const POSIX_FADV_NOREUSE: c_int = 5;

// ---------------------------------------------------------------------------
// open / openat / creat
// ---------------------------------------------------------------------------

/// Internal: actually call openat with explicit args. Shared by `open` and
/// `openat`. Applies the standard upstream tweak of OR-ing in `O_LARGEFILE`
/// (kernel ignores it on 64-bit, but C callers expect it) and re-applies
/// `FD_CLOEXEC` via `fcntl(F_SETFD, …)` to defend against old kernels that
/// honored `O_CLOEXEC` lazily.
///
/// # Safety
/// `filename` must point to a NUL-terminated path.
#[inline]
unsafe fn openat_inner(dirfd: c_int, filename: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    // SAFETY: forwards to the kernel.
    // TODO(thread/cancel): switch to __syscall_cp once mytilus-thread lands.
    let fd = unsafe {
        syscall4(
            SYS_openat,
            dirfd as c_long,
            filename as c_long,
            (flags | O_LARGEFILE) as c_long,
            mode as c_long,
        )
    };
    if fd >= 0 && (flags & O_CLOEXEC) != 0 {
        // SAFETY: belt-and-suspenders mirror of upstream; kernel already
        // sets FD_CLOEXEC for openat with O_CLOEXEC since 2.6.23 but old
        // kernels need the explicit fcntl.
        let _ = unsafe { syscall3(SYS_fcntl, fd, F_SETFD as c_long, FD_CLOEXEC as c_long) };
    }
    // SAFETY: ret() classifies the return.
    unsafe { ret(fd) as c_int }
}

/// `int open(const char *filename, int flags, ...)`
///
/// Variadic third arg is a `mode_t` consumed only when `flags & O_CREAT` is
/// set, or when `flags & O_TMPFILE` matches `O_TMPFILE` exactly.
///
/// # Safety
/// `filename` must point to a NUL-terminated path. If the variadic mode arg
/// is required (per the flag check above), the caller MUST pass it; otherwise
/// it must NOT be passed.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn open(filename: *const c_char, flags: c_int, mut args: ...) -> c_int {
    let mode: mode_t = if (flags & O_CREAT) != 0 || (flags & O_TMPFILE) == O_TMPFILE {
        // SAFETY: caller is contractually required to pass the mode arg here.
        unsafe { args.arg::<mode_t>() }
    } else {
        0
    };
    // SAFETY: filename is asserted NUL-terminated by the caller.
    unsafe { openat_inner(AT_FDCWD, filename, flags, mode) }
}

/// `int openat(int dirfd, const char *filename, int flags, ...)`
///
/// # Safety
/// See [`open`]. `dirfd` must be a valid directory fd or `AT_FDCWD`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn openat(
    dirfd: c_int,
    filename: *const c_char,
    flags: c_int,
    mut args: ...
) -> c_int {
    let mode: mode_t = if (flags & O_CREAT) != 0 || (flags & O_TMPFILE) == O_TMPFILE {
        // SAFETY: caller is contractually required to pass the mode arg here.
        unsafe { args.arg::<mode_t>() }
    } else {
        0
    };
    // SAFETY: filename asserted NUL-terminated.
    unsafe { openat_inner(dirfd, filename, flags, mode) }
}

/// `int creat(const char *filename, mode_t mode)` — convenience wrapper
/// for `open(filename, O_CREAT|O_WRONLY|O_TRUNC, mode)`.
///
/// # Safety
/// See [`open`].
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn creat(filename: *const c_char, mode: mode_t) -> c_int {
    // SAFETY: filename asserted NUL-terminated.
    unsafe { openat_inner(AT_FDCWD, filename, O_CREAT | O_WRONLY | O_TRUNC, mode) }
}

// ---------------------------------------------------------------------------
// fcntl
// ---------------------------------------------------------------------------

/// `int fcntl(int fd, int cmd, ...)`
///
/// Variadic third arg is consumed unconditionally (as `unsigned long`),
/// even for cmds that ignore it — the caller is expected to pass *something*
/// (typically 0). Matches upstream behavior.
///
/// # Safety
/// Caller must pass exactly one variadic arg, of type `unsigned long` or
/// pointer (e.g. `*mut struct flock` for `F_SETLK` / `F_GETLK`).
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn fcntl(fd: c_int, cmd: c_int, mut args: ...) -> c_int {
    // SAFETY: variadic contract.
    let mut arg: c_ulong = unsafe { args.arg::<c_ulong>() };

    if cmd == F_SETFL {
        arg |= O_LARGEFILE as c_ulong;
    }
    // TODO(thread/cancel): F_SETLKW is a cancellation point upstream.
    // TODO(compat): F_GETOWN upstream fakes via F_GETOWN_EX to handle
    //   process-group return ambiguity; we pass through.
    // TODO(compat): F_DUPFD_CLOEXEC upstream falls back to F_DUPFD + F_SETFD
    //   on old kernels; we pass through.

    // SAFETY: forwards to the kernel.
    let r = unsafe { syscall3(SYS_fcntl, fd as c_long, cmd as c_long, arg as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// posix_fadvise / posix_fallocate
// ---------------------------------------------------------------------------
//
// IMPORTANT: both return `errno` directly (positive on failure, 0 on
// success), NOT the standard -1+errno-set convention. Same shape as
// `clock_nanosleep` in mytilus-time.

/// `int posix_fadvise(int fd, off_t base, off_t len, int advice)`.
///
/// Returns 0 on success or a positive `errno`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn posix_fadvise(fd: c_int, base: off_t, len: off_t, advice: c_int) -> c_int {
    // SAFETY: pure kernel call; no caller-supplied pointers.
    let r = unsafe {
        syscall4(
            SYS_fadvise64,
            fd as c_long,
            base as c_long,
            len as c_long,
            advice as c_long,
        )
    };
    -r as c_int
}

/// `int posix_fallocate(int fd, off_t base, off_t len)`.
///
/// Returns 0 on success or a positive `errno`.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn posix_fallocate(fd: c_int, base: off_t, len: off_t) -> c_int {
    // SAFETY: pure kernel call. SYS_fallocate is (fd, mode, offset, len);
    // we always pass mode=0 per upstream.
    let r = unsafe {
        syscall4(
            SYS_fallocate,
            fd as c_long,
            0,
            base as c_long,
            len as c_long,
        )
    };
    -r as c_int
}

#[cfg(test)]
mod tests {
    //! As with the other syscall-wrapping crates, behavior tests can't run
    //! on host: every public function ultimately invokes a `syscallN`, which
    //! is `unimplemented!()` outside aarch64-linux. We assert constant
    //! values (kernel ABI) and syscall numbers — anything that drifts
    //! silently corrupts every caller.

    use super::*;

    #[test]
    fn open_flag_constants_match_linux_abi() {
        // Octal values from arch/aarch64/bits/fcntl.h upstream.
        assert_eq!(O_RDONLY, 0);
        assert_eq!(O_WRONLY, 1);
        assert_eq!(O_RDWR, 2);
        assert_eq!(O_CREAT, 0o100);
        assert_eq!(O_EXCL, 0o200);
        assert_eq!(O_TRUNC, 0o1000);
        assert_eq!(O_APPEND, 0o2000);
        assert_eq!(O_NONBLOCK, 0o4000);
        assert_eq!(O_CLOEXEC, 0o2000000);
        assert_eq!(O_DIRECTORY, 0o40000);
        assert_eq!(O_NOFOLLOW, 0o100000);
        assert_eq!(O_LARGEFILE, 0o400000);
        assert_eq!(O_TMPFILE, 0o20040000);
        // O_TMPFILE includes O_DIRECTORY by design.
        assert_eq!(O_TMPFILE & O_DIRECTORY, O_DIRECTORY);
    }

    #[test]
    fn fcntl_cmds_match_linux_abi() {
        assert_eq!(F_DUPFD, 0);
        assert_eq!(F_GETFD, 1);
        assert_eq!(F_SETFD, 2);
        assert_eq!(F_GETFL, 3);
        assert_eq!(F_SETFL, 4);
        assert_eq!(F_GETLK, 5);
        assert_eq!(F_SETLK, 6);
        assert_eq!(F_SETLKW, 7);
        assert_eq!(F_DUPFD_CLOEXEC, 1030);
        assert_eq!(FD_CLOEXEC, 1);
    }

    #[test]
    fn at_constants_match_linux_abi() {
        assert_eq!(AT_FDCWD, -100);
        assert_eq!(AT_SYMLINK_NOFOLLOW, 0x100);
        assert_eq!(AT_REMOVEDIR, 0x200);
        assert_eq!(AT_EMPTY_PATH, 0x1000);
    }

    #[test]
    fn posix_fadv_constants_match_linux_abi() {
        assert_eq!(POSIX_FADV_NORMAL, 0);
        assert_eq!(POSIX_FADV_RANDOM, 1);
        assert_eq!(POSIX_FADV_SEQUENTIAL, 2);
        assert_eq!(POSIX_FADV_WILLNEED, 3);
        assert_eq!(POSIX_FADV_DONTNEED, 4);
        assert_eq!(POSIX_FADV_NOREUSE, 5);
    }

    #[test]
    fn syscall_numbers_match_aarch64_abi() {
        // From arch/aarch64/bits/syscall.h.in upstream.
        assert_eq!(SYS_fcntl, 25);
        assert_eq!(SYS_fallocate, 47);
        assert_eq!(SYS_openat, 56);
        assert_eq!(SYS_close, 57);
        assert_eq!(SYS_fadvise64, 223);
    }
}
