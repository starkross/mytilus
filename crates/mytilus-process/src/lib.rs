//! `mytilus-process` — pid / uid / gid / process-group / exit / kill /
//! sched_yield (Phase 1).
//!
//! Phase 1 ports the trivial syscall wrappers from `src/unistd/`,
//! `src/process/`, `src/signal/kill.c`, `src/exit/_Exit.c`, and
//! `src/sched/sched_yield.c`:
//!
//! - **Identity**: `getpid`, `getppid`, `getuid`, `geteuid`, `getgid`,
//!   `getegid`. None of these can fail on Linux — kernel returns the value
//!   directly without errno classification.
//! - **Process groups / sessions**: `getsid`, `getpgid`, `getpgrp` (a
//!   userspace wrapper for `getpgid(0)`), `setpgid`, `setsid`. These can
//!   fail (`EPERM`, `ESRCH`, `EINVAL`); they go through `ret()`.
//! - **Signal delivery**: `kill`. Standard `-1+errno` convention.
//! - **Termination**: `_Exit` / `_exit` (POSIX-spelled alias). Calls
//!   `SYS_exit_group` to terminate the whole process; loops on `SYS_exit`
//!   as upstream does, in case the kernel didn't honor exit_group (modern
//!   kernels always do, but we mirror the upstream defense). Marked `!`
//!   so the compiler knows control never returns.
//! - **Scheduler**: `sched_yield`.
//!
//! Deferred to later phases (need real infrastructure):
//! `fork` / `vfork` / `clone` (need `__pthread_self`, lock-reset machinery,
//! TLS save/restore), `posix_spawn` (needs malloc + fork), `wait`/`waitpid`/
//! `wait3`/`wait4`/`waitid` (cancellation points), `execve`/`execv`/
//! `execvp`/`execl*` (need malloc for arg-string handling), `nice` / `getpriority` /
//! `setpriority`, `tgkill` (needs thread struct), `prctl`,  uid/gid setters
//! (`setuid`/`setgid` etc. need the setxid signal broadcast across threads),
//! `getgroups` / `setgroups`, `getrlimit` / `setrlimit`.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

extern crate mytilus_errno;

use mytilus_sys::ctypes::{c_int, c_long, gid_t, pid_t, uid_t};
use mytilus_sys::nr::*;
use mytilus_sys::syscall::{ret, syscall0, syscall1, syscall2};

// ---------------------------------------------------------------------------
// Identity — getpid / getppid / getuid / geteuid / getgid / getegid
// ---------------------------------------------------------------------------
//
// None of these can fail on Linux. Upstream uses raw `__syscall` (no
// `__syscall_ret`); we mirror that — the kernel return is the value, no
// errno path.

/// `pid_t getpid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getpid() -> pid_t {
    // SAFETY: pure kernel call.
    unsafe { syscall0(SYS_getpid) as pid_t }
}

/// `pid_t getppid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getppid() -> pid_t {
    // SAFETY: pure kernel call.
    unsafe { syscall0(SYS_getppid) as pid_t }
}

/// `uid_t getuid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getuid() -> uid_t {
    // SAFETY: pure kernel call.
    unsafe { syscall0(SYS_getuid) as uid_t }
}

/// `uid_t geteuid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn geteuid() -> uid_t {
    // SAFETY: pure kernel call.
    unsafe { syscall0(SYS_geteuid) as uid_t }
}

/// `gid_t getgid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getgid() -> gid_t {
    // SAFETY: pure kernel call.
    unsafe { syscall0(SYS_getgid) as gid_t }
}

/// `gid_t getegid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getegid() -> gid_t {
    // SAFETY: pure kernel call.
    unsafe { syscall0(SYS_getegid) as gid_t }
}

// ---------------------------------------------------------------------------
// Process groups / sessions — getsid / getpgid / getpgrp / setpgid / setsid
// ---------------------------------------------------------------------------

/// `pid_t getsid(pid_t pid)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getsid(pid: pid_t) -> pid_t {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall1(SYS_getsid, pid as c_long) };
    // SAFETY: ret() classifies; success returns the session id.
    unsafe { ret(r) as pid_t }
}

/// `pid_t getpgid(pid_t pid)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getpgid(pid: pid_t) -> pid_t {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall1(SYS_getpgid, pid as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as pid_t }
}

/// `pid_t getpgrp(void)` — POSIX-spelled alias for `getpgid(0)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn getpgrp() -> pid_t {
    // SAFETY: pure kernel call. Upstream uses `__syscall` (no error
    // classification) since `getpgid(0)` can't fail in this form.
    unsafe { syscall1(SYS_getpgid, 0) as pid_t }
}

/// `int setpgid(pid_t pid, pid_t pgid)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn setpgid(pid: pid_t, pgid: pid_t) -> c_int {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall2(SYS_setpgid, pid as c_long, pgid as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `pid_t setsid(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn setsid() -> pid_t {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall0(SYS_setsid) };
    // SAFETY: ret() classifies; success returns the new session id.
    unsafe { ret(r) as pid_t }
}

// ---------------------------------------------------------------------------
// Signal delivery — kill
// ---------------------------------------------------------------------------

/// `int kill(pid_t pid, int sig)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn kill(pid: pid_t, sig: c_int) -> c_int {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall2(SYS_kill, pid as c_long, sig as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// Termination — _Exit / _exit / exit_group
// ---------------------------------------------------------------------------

/// `_Noreturn void _Exit(int status)`.
///
/// Calls `SYS_exit_group` to terminate the whole process. Loops on
/// `SYS_exit` as a defensive fallback for old kernels that didn't honor
/// `exit_group` (modern kernels always do; we mirror upstream).
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn _Exit(status: c_int) -> ! {
    // SAFETY: pure kernel calls; nothing returns.
    unsafe {
        syscall1(SYS_exit_group, status as c_long);
        loop {
            syscall1(SYS_exit, status as c_long);
        }
    }
}

/// `_Noreturn void _exit(int status)` — POSIX-spelled alias for `_Exit`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn _exit(status: c_int) -> ! {
    _Exit(status)
}

// ---------------------------------------------------------------------------
// Scheduler — sched_yield
// ---------------------------------------------------------------------------

/// `int sched_yield(void)`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub extern "C" fn sched_yield() -> c_int {
    // SAFETY: pure kernel call.
    let r = unsafe { syscall0(SYS_sched_yield) };
    // SAFETY: ret() classifies; this syscall almost never fails but per
    // POSIX it can return EINVAL for invalid scheduling policies on some
    // archs. ret() handles that uniformly.
    unsafe { ret(r) as c_int }
}

#[cfg(test)]
mod tests {
    //! Behavior tests can't exercise the syscall path on host (stubs panic).
    //! We verify the syscall-number constants match the kernel ABI; drift
    //! silently breaks every consumer.

    use super::*;

    #[test]
    fn syscall_numbers_match_aarch64_abi() {
        // From arch/aarch64/bits/syscall.h.in upstream.
        assert_eq!(SYS_exit, 93);
        assert_eq!(SYS_exit_group, 94);
        assert_eq!(SYS_sched_yield, 124);
        assert_eq!(SYS_kill, 129);
        assert_eq!(SYS_setpgid, 154);
        assert_eq!(SYS_getpgid, 155);
        assert_eq!(SYS_getsid, 156);
        assert_eq!(SYS_setsid, 157);
        assert_eq!(SYS_getpid, 172);
        assert_eq!(SYS_getppid, 173);
        assert_eq!(SYS_getuid, 174);
        assert_eq!(SYS_geteuid, 175);
        assert_eq!(SYS_getgid, 176);
        assert_eq!(SYS_getegid, 177);
    }
}
