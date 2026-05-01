//! `mytilus-time` — clock, time, sleep.
//!
//! Phase 1 ports the syscall wrappers from `src/time/` upstream:
//! `clock_gettime` / `__clock_gettime` (+ POSIX wrappers `time`,
//! `gettimeofday`), `clock_settime`, `clock_getres`, `clock_nanosleep` /
//! `__clock_nanosleep`, `nanosleep`. Plus the `struct timespec` /
//! `struct timeval` definitions and the `CLOCK_*` constants.
//!
//! Deferred to later phases (need malloc, fcntl, or substantial table data):
//! `mktime`, `gmtime`, `localtime`, `strftime`, `strptime`, `ctime`,
//! `asctime`, `difftime`, `timer_*`, TZif parser, `__tz` machinery.
//!
//! TODO(perf, vDSO): upstream `clock_gettime` first tries the kernel-provided
//! vDSO function (`__kernel_clock_gettime`) and falls back to `svc` only if
//! that fails. We always go through `svc` for now. The vDSO path needs an
//! auxv reader (`AT_SYSINFO_EHDR`) to find the vDSO base — wire it once
//! `mytilus-startup` parses auxv.
//!
//! TODO(thread/cancel): upstream `clock_nanosleep` and `nanosleep` use
//! `__syscall_cp` (cancellation-point variant). We use plain `svc`. Switch
//! when `mytilus-thread`'s `__syscall_cp` asm lands.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

// Force-link mytilus-errno so `__errno_location` is in the final binary;
// `mytilus-sys::syscall::ret` resolves to it via `extern "C"`.
extern crate mytilus_errno;

use mytilus_sys::ctypes::{c_int, c_long, c_void, clockid_t, suseconds_t, time_t};
use mytilus_sys::errno_raw::EINVAL;
use mytilus_sys::nr::*;
use mytilus_sys::syscall::{ret, syscall2, syscall4};

// ---------------------------------------------------------------------------
// FFI types
// ---------------------------------------------------------------------------

/// `struct timespec` — POSIX nanosecond resolution time. LP64 layout:
/// 16 bytes, 8-byte aligned, `tv_sec` at offset 0, `tv_nsec` at offset 8.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct timespec {
    pub tv_sec: time_t,
    pub tv_nsec: c_long,
}

/// `struct timeval` — microsecond resolution, used by `gettimeofday` and a
/// handful of socket options (`SO_RCVTIMEO`/`SO_SNDTIMEO`). LP64 layout:
/// 16 bytes, `tv_sec` at offset 0, `tv_usec` at offset 8.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct timeval {
    pub tv_sec: time_t,
    pub tv_usec: suseconds_t,
}

// ---------------------------------------------------------------------------
// Constants — kernel ABI for AArch64 Linux. Values match `<bits/time.h>` and
// `<linux/time.h>` upstream.
// ---------------------------------------------------------------------------

pub const CLOCK_REALTIME: clockid_t = 0;
pub const CLOCK_MONOTONIC: clockid_t = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: clockid_t = 2;
pub const CLOCK_THREAD_CPUTIME_ID: clockid_t = 3;
pub const CLOCK_MONOTONIC_RAW: clockid_t = 4;
pub const CLOCK_REALTIME_COARSE: clockid_t = 5;
pub const CLOCK_MONOTONIC_COARSE: clockid_t = 6;
pub const CLOCK_BOOTTIME: clockid_t = 7;
pub const CLOCK_REALTIME_ALARM: clockid_t = 8;
pub const CLOCK_BOOTTIME_ALARM: clockid_t = 9;
pub const CLOCK_TAI: clockid_t = 11;

/// Flag for `clock_nanosleep`: treat `req` as an absolute deadline rather
/// than a relative duration.
pub const TIMER_ABSTIME: c_int = 1;

// ---------------------------------------------------------------------------
// clock_gettime / clock_settime / clock_getres
// ---------------------------------------------------------------------------

/// `int clock_gettime(clockid_t clk, struct timespec *ts)`
///
/// Internal name; upstream is `weak_alias(__clock_gettime, clock_gettime)`.
///
/// # Safety
/// `ts` must point to a writable `timespec`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn __clock_gettime(clk: clockid_t, ts: *mut timespec) -> c_int {
    // SAFETY: forwards to the kernel; `ts` is asserted writable by the caller.
    let r = unsafe { syscall2(SYS_clock_gettime, clk as c_long, ts as c_long) };
    // SAFETY: ret() classifies the return.
    unsafe { ret(r) as c_int }
}

/// # Safety
/// See [`__clock_gettime`].
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn clock_gettime(clk: clockid_t, ts: *mut timespec) -> c_int {
    // SAFETY: forwarded.
    unsafe { __clock_gettime(clk, ts) }
}

/// `int clock_settime(clockid_t clk, const struct timespec *ts)`
///
/// # Safety
/// `ts` must point to a readable `timespec`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn clock_settime(clk: clockid_t, ts: *const timespec) -> c_int {
    // SAFETY: forwards to the kernel.
    let r = unsafe { syscall2(SYS_clock_settime, clk as c_long, ts as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int clock_getres(clockid_t clk, struct timespec *ts)`
///
/// # Safety
/// `ts` may be NULL (in which case the kernel just validates `clk`).
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn clock_getres(clk: clockid_t, ts: *mut timespec) -> c_int {
    // SAFETY: forwards to the kernel.
    let r = unsafe { syscall2(SYS_clock_getres, clk as c_long, ts as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// clock_nanosleep / nanosleep
// ---------------------------------------------------------------------------
//
// IMPORTANT: `clock_nanosleep` returns `errno` directly (positive on
// failure, 0 on success), NOT the standard -1+errno-set convention.
// `nanosleep` uses the standard convention.

/// `int __clock_nanosleep(clockid_t clk, int flags, const struct timespec *req,
///                        struct timespec *rem)` — internal name; upstream
/// has `weak_alias(__clock_nanosleep, clock_nanosleep)`.
///
/// Returns 0 on success or a positive `errno` on failure (POSIX convention
/// for this function specifically).
///
/// # Safety
/// `req` must point to a readable `timespec`. `rem` may be NULL or point to
/// a writable `timespec`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn __clock_nanosleep(
    clk: clockid_t,
    flags: c_int,
    req: *const timespec,
    rem: *mut timespec,
) -> c_int {
    // Upstream rejects CLOCK_THREAD_CPUTIME_ID outright; the kernel won't
    // honor sleeps on the current thread's own CPU clock anyway.
    if clk == CLOCK_THREAD_CPUTIME_ID {
        return EINVAL;
    }
    // SAFETY: forwards to the kernel.
    // TODO(thread/cancel): switch to __syscall_cp once mytilus-thread lands.
    let r = unsafe {
        syscall4(
            SYS_clock_nanosleep,
            clk as c_long,
            flags as c_long,
            req as c_long,
            rem as c_long,
        )
    };
    // The kernel returns -errno; convert to positive errno (or 0 on success).
    -r as c_int
}

/// # Safety
/// See [`__clock_nanosleep`].
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn clock_nanosleep(
    clk: clockid_t,
    flags: c_int,
    req: *const timespec,
    rem: *mut timespec,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { __clock_nanosleep(clk, flags, req, rem) }
}

/// `int nanosleep(const struct timespec *req, struct timespec *rem)`
///
/// Returns 0 on success or -1 with `errno` set. Wraps `clock_nanosleep`
/// against `CLOCK_REALTIME` per upstream.
///
/// # Safety
/// `req` must point to a readable `timespec`. `rem` may be NULL or point to
/// a writable `timespec`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn nanosleep(req: *const timespec, rem: *mut timespec) -> c_int {
    // Upstream: __syscall_ret(-__clock_nanosleep(CLOCK_REALTIME, 0, req, rem)).
    // clock_nanosleep returns positive errno on failure; negate to put it
    // back in the -errno form that ret() expects, then ret() will set errno
    // and return -1.
    // SAFETY: forwarded.
    let positive = unsafe { __clock_nanosleep(CLOCK_REALTIME, 0, req, rem) };
    // SAFETY: ret() handles the C-ABI conversion.
    unsafe { ret(-(positive as c_long)) as c_int }
}

// ---------------------------------------------------------------------------
// gettimeofday / time — userspace wrappers around clock_gettime(REALTIME)
// ---------------------------------------------------------------------------

/// `int gettimeofday(struct timeval *tv, void *tz)`
///
/// `tz` is ignored (it's deprecated; POSIX leaves its meaning unspecified).
/// Always returns 0; if `tv` is NULL the call is a no-op.
///
/// # Safety
/// `tv` may be NULL or point to a writable `timeval`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn gettimeofday(tv: *mut timeval, _tz: *mut c_void) -> c_int {
    if tv.is_null() {
        return 0;
    }
    let mut ts = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: ts is a stack local; clock_gettime writes into it.
    unsafe {
        clock_gettime(CLOCK_REALTIME, &mut ts);
        (*tv).tv_sec = ts.tv_sec;
        (*tv).tv_usec = (ts.tv_nsec / 1000) as suseconds_t;
    }
    0
}

/// `time_t time(time_t *t)` — current wall-clock seconds since the epoch.
///
/// # Safety
/// `t` may be NULL or point to a writable `time_t`.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn time(t: *mut time_t) -> time_t {
    let mut ts = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: ts is a stack local. Upstream ignores the return code because
    // CLOCK_REALTIME never actually fails on Linux; we mirror that.
    unsafe {
        __clock_gettime(CLOCK_REALTIME, &mut ts);
        if !t.is_null() {
            *t = ts.tv_sec;
        }
    }
    ts.tv_sec
}

#[cfg(test)]
mod tests {
    //! Behavioral tests can't run on host: every public function ultimately
    //! invokes a `syscallN`, which is `unimplemented!()` outside aarch64-linux.
    //! What we *can* assert here is that the FFI struct layouts and the
    //! constants match the kernel ABI — drift in either silently corrupts
    //! every caller, so a clean compile-time / unit-test signal matters.

    use core::mem::{align_of, offset_of, size_of};

    use super::*;

    // ---- struct timespec ------------------------------------------------

    #[test]
    fn timespec_layout_matches_lp64_abi() {
        assert_eq!(size_of::<timespec>(), 16);
        assert_eq!(align_of::<timespec>(), 8);
        assert_eq!(offset_of!(timespec, tv_sec), 0);
        assert_eq!(offset_of!(timespec, tv_nsec), 8);
    }

    // ---- struct timeval -------------------------------------------------

    #[test]
    fn timeval_layout_matches_lp64_abi() {
        assert_eq!(size_of::<timeval>(), 16);
        assert_eq!(align_of::<timeval>(), 8);
        assert_eq!(offset_of!(timeval, tv_sec), 0);
        assert_eq!(offset_of!(timeval, tv_usec), 8);
    }

    // ---- clock id constants --------------------------------------------

    #[test]
    fn clock_constants_match_linux_abi() {
        // From <linux/time.h>; these are part of the kernel ABI and
        // never change.
        assert_eq!(CLOCK_REALTIME, 0);
        assert_eq!(CLOCK_MONOTONIC, 1);
        assert_eq!(CLOCK_PROCESS_CPUTIME_ID, 2);
        assert_eq!(CLOCK_THREAD_CPUTIME_ID, 3);
        assert_eq!(CLOCK_MONOTONIC_RAW, 4);
        assert_eq!(CLOCK_REALTIME_COARSE, 5);
        assert_eq!(CLOCK_MONOTONIC_COARSE, 6);
        assert_eq!(CLOCK_BOOTTIME, 7);
        assert_eq!(CLOCK_REALTIME_ALARM, 8);
        assert_eq!(CLOCK_BOOTTIME_ALARM, 9);
        assert_eq!(CLOCK_TAI, 11);
    }

    #[test]
    fn timer_abstime_matches_linux_abi() {
        assert_eq!(TIMER_ABSTIME, 1);
    }

    // ---- syscall numbers -----------------------------------------------

    #[test]
    fn syscall_numbers_match_aarch64_abi() {
        // From arch/aarch64/bits/syscall.h.in upstream.
        assert_eq!(SYS_nanosleep, 101);
        assert_eq!(SYS_clock_settime, 112);
        assert_eq!(SYS_clock_gettime, 113);
        assert_eq!(SYS_clock_getres, 114);
        assert_eq!(SYS_clock_nanosleep, 115);
        assert_eq!(SYS_gettimeofday, 169);
    }
}
