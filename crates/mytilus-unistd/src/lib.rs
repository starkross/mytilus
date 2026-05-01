//! `mytilus-unistd` — `<unistd.h>` Phase 1 subset: sleep family, fd shuffle,
//! sync family.
//!
//! Phase 1 ports:
//! - `sleep` / `usleep` — userspace wrappers calling `mytilus_time::nanosleep`
//!   (first **cross-crate-symbol** consumer in the workspace).
//! - `pause` — uses `ppoll(0,0,0,0)` since aarch64 has no `SYS_pause`.
//! - `dup` / `dup2` / `dup3` — fd shuffle. aarch64 routes `dup2` through
//!   `SYS_dup3` (no `SYS_dup2`).
//! - `getpagesize` — currently hardcoded to 4096 (matches PLAN.md's
//!   single-target stance); TODO to read `AT_PAGESZ` from auxv.
//! - `sync` / `fsync` / `fdatasync` — direct syscall wrappers.
//!
//! Deferred: `read`/`write`/`pread`/`pwrite`/`readv`/`writev` (need
//! cancellation + buffer hardening), `lseek`, `pipe`/`pipe2`, `access`/
//! `faccessat`, `chdir`/`fchdir`/`chroot`, `getcwd`, `link`/`unlink`/
//! `symlink`/`*at` family, `getopt`/`getlogin`/`gethostname`, `fork`/
//! `execve`/`vfork`, `nice`, `alarm`, `tcsetpgrp`, the user/group lookups,
//! and a long tail of others. Most need malloc, fcntl, signals, or threads.
//!
//! TODO(thread/cancel): `pause`, `fsync`, `fdatasync` are cancellation
//! points upstream (use `__syscall_cp`). We use plain `svc`. Switch when
//! `mytilus-thread`'s asm lands.
//!
//! TODO(auxv): `getpagesize` returns 4096 unconditionally. When
//! `mytilus-startup` parses auxv we should swap it for the real `AT_PAGESZ`
//! value (which on aarch64 can be 4 KB / 16 KB / 64 KB depending on kernel
//! page-size config).
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

extern crate mytilus_errno;

use mytilus_sys::ctypes::{c_int, c_long, c_uint, time_t};
use mytilus_sys::nr::*;
use mytilus_sys::syscall::{is_err, ret, syscall0, syscall1, syscall2, syscall3, syscall4};
use mytilus_time::{nanosleep, timespec};

// ---------------------------------------------------------------------------
// sleep / usleep — first cross-crate-symbol consumers (call `nanosleep`)
// ---------------------------------------------------------------------------

/// `unsigned sleep(unsigned seconds)`.
///
/// Returns 0 on full sleep, or the remaining seconds if interrupted by a
/// signal (via `nanosleep`'s residual-time output).
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn sleep(seconds: c_uint) -> c_uint {
    let mut tv = timespec {
        tv_sec: seconds as time_t,
        tv_nsec: 0,
    };
    // SAFETY: `tv` is a stack local; nanosleep reads `req` and writes
    // residual into `rem` (we pass the same slot for both, matching upstream).
    let r = unsafe { nanosleep(&tv, &mut tv) };
    if r != 0 {
        // Interrupted: tv holds the remaining time.
        tv.tv_sec as c_uint
    } else {
        0
    }
}

/// `int usleep(useconds_t useconds)`.
///
/// `useconds_t` is `unsigned int` per glibc/musl on Linux.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn usleep(useconds: c_uint) -> c_int {
    let useconds = useconds as u64;
    let mut tv = timespec {
        tv_sec: (useconds / 1_000_000) as time_t,
        tv_nsec: ((useconds % 1_000_000) * 1_000) as c_long,
    };
    // SAFETY: stack-local timespec.
    unsafe { nanosleep(&tv, &mut tv) }
}

// ---------------------------------------------------------------------------
// pause — ppoll(0,0,0,0)
// ---------------------------------------------------------------------------

/// `int pause(void)` — wait for any signal that's not ignored.
///
/// AArch64 has no `SYS_pause`; we use `ppoll(NULL, 0, NULL, NULL)` which is
/// the documented equivalent (waits forever on a zero-length fd set).
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn pause() -> c_int {
    // SAFETY: pure kernel call; no caller-supplied pointers (all NULL).
    // TODO(thread/cancel): switch to __syscall_cp once mytilus-thread lands.
    let r = unsafe { syscall4(SYS_ppoll, 0, 0, 0, 0) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// dup / dup2 / dup3
// ---------------------------------------------------------------------------

/// `int dup(int fd)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn dup(fd: c_int) -> c_int {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall1(SYS_dup, fd as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int dup3(int old, int new, int flags)`.
///
/// Internal alias `__dup3` exposed for ABI compatibility (upstream has a
/// weak alias). On aarch64 there's no `SYS_dup2`, so the EBUSY-retry loop
/// upstream guards against on other archs is unneeded here — `SYS_dup3`
/// handles the atomicity directly.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn __dup3(old: c_int, new: c_int, flags: c_int) -> c_int {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall3(SYS_dup3, old as c_long, new as c_long, flags as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn dup3(old: c_int, new: c_int, flags: c_int) -> c_int {
    __dup3(old, new, flags)
}

/// `int dup2(int old, int new)`.
///
/// AArch64 has no `SYS_dup2`. We mirror upstream's "no SYS_dup2" branch:
/// when `old == new`, validate `old` via `fcntl(F_GETFD)` and return it
/// unchanged; otherwise route through `SYS_dup3` with flags=0.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn dup2(old: c_int, new: c_int) -> c_int {
    // Per POSIX: dup2(fd, fd) validates fd and returns it unchanged.
    const F_GETFD: c_long = 1;
    if old == new {
        // SAFETY: fcntl(F_GETFD) is a pure kernel call.
        let r = unsafe { syscall2(SYS_fcntl, old as c_long, F_GETFD) };
        if !is_err(r) {
            return old;
        }
        // SAFETY: ret() classifies (sets errno + returns -1).
        return unsafe { ret(r) as c_int };
    }
    // SAFETY: pure kernel call.
    let r = unsafe { syscall3(SYS_dup3, old as c_long, new as c_long, 0) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// getpagesize
// ---------------------------------------------------------------------------

/// `int getpagesize(void)`.
///
/// TODO(auxv): hardcoded 4096 until `mytilus-startup` reads `AT_PAGESZ`.
/// PLAN.md commits to 4 KB pages on aarch64; if we ever care about 16 KB
/// or 64 KB kernel-page builds, this needs to read the real value at
/// startup.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getpagesize() -> c_int {
    4096
}

// ---------------------------------------------------------------------------
// sync / fsync / fdatasync
// ---------------------------------------------------------------------------

/// `void sync(void)`.
///
/// Always succeeds on Linux (the syscall returns void).
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn sync() {
    // SAFETY: no args; kernel ignores return.
    let _ = unsafe { syscall0(SYS_sync) };
}

/// `int fsync(int fd)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn fsync(fd: c_int) -> c_int {
    // SAFETY: pure kernel call.
    // TODO(thread/cancel): cancellation point upstream.
    let r = unsafe { syscall1(SYS_fsync, fd as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int fdatasync(int fd)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn fdatasync(fd: c_int) -> c_int {
    // SAFETY: pure kernel call.
    // TODO(thread/cancel): cancellation point upstream.
    let r = unsafe { syscall1(SYS_fdatasync, fd as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

#[cfg(test)]
mod tests {
    //! Behavior tests can't exercise the syscall path (stubs panic on host).
    //! We assert constant values + cross-crate `mytilus-time` linkage.

    use super::*;

    #[test]
    fn syscall_numbers_match_aarch64_abi() {
        assert_eq!(SYS_dup, 23);
        assert_eq!(SYS_dup3, 24);
        assert_eq!(SYS_ppoll, 73);
        assert_eq!(SYS_sync, 81);
        assert_eq!(SYS_fsync, 82);
        assert_eq!(SYS_fdatasync, 83);
    }

    #[test]
    fn getpagesize_returns_4k() {
        assert_eq!(getpagesize(), 4096);
    }

    /// Verify `mytilus-time` items are nameable from this crate. The actual
    /// `nanosleep` call would panic on host (svc stub), so we only check
    /// the type can be constructed — which is the build-time signal that
    /// the cross-crate dep + symbol resolution path is wired correctly.
    #[test]
    fn cross_crate_dep_links() {
        let _ts = timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // Reference the `nanosleep` symbol so it ends up in the dep graph.
        // Cast through a fn pointer; never call.
        let f: unsafe extern "C" fn(*const timespec, *mut timespec) -> c_int = nanosleep;
        // Force the compiler to keep `f` (otherwise it gets dead-stripped
        // before linking, defeating the point of this test).
        let _ = core::hint::black_box(f);
    }
}
