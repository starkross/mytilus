//! `mytilus-unistd` — `<unistd.h>`: sleep, fd shuffle, sync, basic I/O.
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
//! Phase 2 ports (basic I/O):
//! - `read` / `write` — 3-arg cancellation-point syscalls.
//! - `close` — with the POSIX-2008 `EINTR → success` mapping (Linux always
//!   closes the fd, even on EINTR).
//! - `lseek` / `__lseek` — `SYS_lseek` directly (no 32-bit `_llseek`
//!   splitting needed on LP64).
//!
//! Deferred: `pread`/`pwrite`/`readv`/`writev`, `pipe`/`pipe2`, `access`/
//! `faccessat`, `chdir`/`fchdir`/`chroot`, `getcwd`, `link`/`unlink`/
//! `symlink`/`*at` family, `getopt`/`getlogin`/`gethostname`, `fork`/
//! `execve`/`vfork`, `nice`, `alarm`, `tcsetpgrp`, the user/group lookups,
//! and a long tail of others. Most need malloc, fcntl, signals, or threads.
//!
//! Cancellation: `pause`, `fsync`, `fdatasync`, `read`, `write`, `close`
//! are cancellation points upstream. They route through
//! `mytilus_sys::syscall::syscall_cp_N`, which goes through
//! `__syscall_cp_asm`. Until `mytilus-thread` provides a real cancel flag,
//! the asm's cancel-branch is never taken — the call sites are correctly
//! flagged so the swap to real cancellation is local to the asm/wrapper
//! layer.
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

use mytilus_sys::ctypes::{c_int, c_long, c_uint, c_void, off_t, size_t, ssize_t, time_t};
use mytilus_sys::errno_raw::EINTR;
use mytilus_sys::nr::*;
use mytilus_sys::syscall::{
    is_err, ret, syscall0, syscall1, syscall2, syscall3, syscall_cp1, syscall_cp3, syscall_cp4,
};
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
    // SAFETY: cancellation point — routed through syscall_cp4. All four
    // ppoll args are NULL/0 since `pause` is "wait forever on no fds".
    let r = unsafe { syscall_cp4(SYS_ppoll, 0, 0, 0, 0) };
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
    // SAFETY: cancellation point — routed through syscall_cp1.
    let r = unsafe { syscall_cp1(SYS_fsync, fd as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int fdatasync(int fd)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn fdatasync(fd: c_int) -> c_int {
    // SAFETY: cancellation point — routed through syscall_cp1.
    let r = unsafe { syscall_cp1(SYS_fdatasync, fd as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// read / write / close / lseek  (Phase 2 — basic I/O)
// ---------------------------------------------------------------------------
//
// All four are cancellation points upstream — they route through
// `syscall_cp_N`. Until `mytilus-thread` provides a real cancel flag, the
// asm's cancel-branch is dead code (DUMMY_CANCEL = 0); the swap to real
// cancellation is local to `mytilus-sys::syscall`.

/// `ssize_t read(int fd, void *buf, size_t count)`.
///
/// # Safety
/// `buf` must be writable for at least `count` bytes.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t {
    // SAFETY: cancellation point — routed through syscall_cp3.
    let r = unsafe { syscall_cp3(SYS_read, fd as c_long, buf as c_long, count as c_long) };
    // SAFETY: ret() classifies; success returns the byte count, fits in
    // ssize_t (= isize = i64 on LP64).
    unsafe { ret(r) as ssize_t }
}

/// `ssize_t write(int fd, const void *buf, size_t count)`.
///
/// # Safety
/// `buf` must be readable for at least `count` bytes.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t {
    // SAFETY: cancellation point — routed through syscall_cp3.
    let r = unsafe { syscall_cp3(SYS_write, fd as c_long, buf as c_long, count as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as ssize_t }
}

/// `int close(int fd)`.
///
/// Per POSIX-2008, EINTR on `close` is mapped to success: Linux always
/// closes the fd even when the syscall returns EINTR, so reporting failure
/// would lead callers to retry on a now-stale fd. Mirrors upstream.
///
/// TODO(aio): upstream calls a weak `__aio_close(fd)` first to cancel any
/// pending aio against the fd. We skip it — `__aio_close` is a no-op weak
/// alias unless `mytilus-aio` is in use, and `mytilus-aio` is empty.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn close(fd: c_int) -> c_int {
    // SAFETY: cancellation point — routed through syscall_cp1.
    let r = unsafe { syscall_cp1(SYS_close, fd as c_long) };
    // EINTR → 0 (success) per POSIX-2008.
    if r == -(EINTR as c_long) {
        return 0;
    }
    // SAFETY: ret() classifies the remaining cases.
    unsafe { ret(r) as c_int }
}

/// `off_t lseek(int fd, off_t offset, int whence)`.
///
/// Internal alias `__lseek` exposed for ABI compat (upstream has a weak
/// alias). On aarch64-LP64 there's no `SYS__llseek` argument-splitting —
/// `SYS_lseek` takes a 64-bit offset directly.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn __lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall3(SYS_lseek, fd as c_long, offset as c_long, whence as c_long) };
    // SAFETY: ret() classifies; success returns the new file offset, which
    // fits in off_t (= i64).
    unsafe { ret(r) as off_t }
}

#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t {
    __lseek(fd, offset, whence)
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
        assert_eq!(SYS_lseek, 62);
        assert_eq!(SYS_read, 63);
        assert_eq!(SYS_write, 64);
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
