//! AArch64 syscall stubs.
//!
//! ABI: `x8` = syscall number, `x0`..`x5` = args, return in `x0`.
//! Errors come back as `-errno` in the range `-4096..-1`; classification lives
//! in [`is_err`] so call sites stay simple.
//!
//! Each `syscallN` issues a `svc #0` directly. The kernel preserves all
//! registers except `x0` per the Linux AArch64 syscall ABI; `rustc`'s default
//! inline-asm semantics assume memory and flags may be modified, so we don't
//! need to mark either as clobbered.
//!
//! `syscall_cp_N` (the cancellation-point variant) is **also here**. It
//! routes through `__syscall_cp_asm` (assembly in `asm/syscall_cp.s`) so a
//! future cancel-handler can recognise the exact PC range. The eventual home
//! for the asm + cancel framework is `mytilus-thread`; we host it in
//! `mytilus-sys` for now so the `cp` wrappers sit alongside the plain ones.
//! Until `mytilus-thread` provides a real `__pthread_self()->cancel`, the
//! wrappers pass `&DUMMY_CANCEL` (= 0) so the cancel-branch never fires —
//! `syscall_cp_N` is functionally identical to `syscall_N` but flagged at
//! every cancellable site so the swap to real cancellation is local to
//! this file.
//!
//! Host fallback: when built for any non-aarch64-linux target (including the
//! macOS / x86_64-linux dev hosts we use for `cargo check` / `cargo test`),
//! every `syscallN` panics with `unimplemented!()`. That keeps the workspace
//! buildable on the host while making accidental host execution loud. Real
//! syscall coverage runs under `qemu-aarch64` (TODO: wire `task test:qemu`).

use crate::ctypes::{c_int, c_long};

/// Issue a syscall with no arguments.
///
/// # Safety
/// Caller is responsible for using a kernel-valid syscall number `n` and
/// ensuring side effects of the call are sound for the surrounding program
/// state.
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall0(n: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: svc #0 transitions to the kernel; x0 is read on return; no
    // memory or stack assumptions beyond what the caller validated.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall0 only runs on aarch64-linux")
    }
}

/// Issue a syscall with one argument.
///
/// # Safety
/// See [`syscall0`].
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall1(n: c_long, a0: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall0.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            inlateout("x0") a0 => ret,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall1 only runs on aarch64-linux")
    }
}

/// Issue a syscall with two arguments.
///
/// # Safety
/// See [`syscall0`].
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall2(n: c_long, a0: c_long, a1: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall0.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall2 only runs on aarch64-linux")
    }
}

/// Issue a syscall with three arguments.
///
/// # Safety
/// See [`syscall0`].
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall3(n: c_long, a0: c_long, a1: c_long, a2: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall0.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall3 only runs on aarch64-linux")
    }
}

/// Issue a syscall with four arguments.
///
/// # Safety
/// See [`syscall0`].
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall4(n: c_long, a0: c_long, a1: c_long, a2: c_long, a3: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall0.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall4 only runs on aarch64-linux")
    }
}

/// Issue a syscall with five arguments.
///
/// # Safety
/// See [`syscall0`].
#[inline]
#[allow(unused_variables, clippy::too_many_arguments)]
pub unsafe fn syscall5(
    n: c_long,
    a0: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall0.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall5 only runs on aarch64-linux")
    }
}

/// Issue a syscall with six arguments (the maximum on AArch64 Linux).
///
/// # Safety
/// See [`syscall0`].
#[inline]
#[allow(unused_variables, clippy::too_many_arguments)]
pub unsafe fn syscall6(
    n: c_long,
    a0: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
    a5: c_long,
) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall0.
    unsafe {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack),
        );
        ret
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall6 only runs on aarch64-linux")
    }
}

/// Result classifier: kernel returns negative errno in `-4096..-1`.
#[inline]
pub fn is_err(rc: c_long) -> bool {
    (rc as u64) >= (-4096_i64 as u64)
}

// `__errno_location` is provided by `mytilus-errno`. We declare it `extern "C"`
// so this crate can stay free of a Rust-level dep on errno; the linker
// resolves the symbol at final-link time. (mytilus-sys deliberately sits at
// the bottom of the dep graph.)
unsafe extern "C" {
    fn __errno_location() -> *mut c_int;
}

/// Apply the C-ABI return-value convention to a raw kernel return:
/// if `r` is `-errno`, set `errno` and return `-1`; otherwise pass through.
///
/// For pointer-returning syscalls (`mmap`, `mremap`), callers cast the
/// result to `*mut c_void` — `-1 as *mut c_void` is the same bit pattern
/// as `MAP_FAILED`, so the convention works for both shapes.
///
/// # Safety
/// Must run on a target where `__errno_location` is linked in (i.e. with
/// `mytilus-errno` in the final binary).
#[inline]
pub unsafe fn ret(r: c_long) -> c_long {
    if is_err(r) {
        // SAFETY: __errno_location is contractually a valid TLS pointer.
        unsafe {
            *__errno_location() = -r as c_int;
        }
        -1
    } else {
        r
    }
}

// ---------------------------------------------------------------------------
// Cancellation-point syscalls — `syscall_cp_N` family.
// ---------------------------------------------------------------------------
//
// Same shape as `syscall_N`, but routes through the assembly stub
// `__syscall_cp_asm` whose PC range is recognised by a future cancel-handler.
// Until `mytilus-thread` exposes `__pthread_self()->cancel`, every call
// passes `&DUMMY_CANCEL` (= 0), so the in-asm `cbnz` branch to `__cancel`
// is provably never taken. The wrappers are thus functionally identical
// to the plain `syscall_N` equivalents — but every cancellation point in
// the workspace is now flagged at the call site, so swapping in real
// cancellation later is local to this file.

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
core::arch::global_asm!(include_str!("../asm/syscall_cp.s"));

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
unsafe extern "C" {
    /// `long __syscall_cp_asm(int *cancel, long nr, long u, long v, long w,
    ///                        long x, long y, long z)`. Defined in
    /// `asm/syscall_cp.s`.
    fn __syscall_cp_asm(
        cancel: *mut c_int,
        nr: c_long,
        u: c_long,
        v: c_long,
        w: c_long,
        x: c_long,
        y: c_long,
        z: c_long,
    ) -> c_long;
}

/// Per-thread cancel flag. Real implementation will be
/// `&__pthread_self()->cancel`; until that lands, we use a static `0` so
/// the cancel branch is always skipped.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
static mut DUMMY_CANCEL: c_int = 0;

/// `__cancel` skeleton — the asm `b __cancel` target, taken when the
/// cancel flag is non-zero. With `DUMMY_CANCEL` permanently 0, this
/// branch is unreachable in practice. We still need to provide the
/// symbol so the asm links; if it ever does fire, terminate hard rather
/// than continue with a corrupt thread state.
///
/// Real impl (`mytilus-thread`) will throw via the cleanup-handler chain
/// up to `pthread_exit(PTHREAD_CANCELED)`.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
#[no_mangle]
pub extern "C" fn __cancel() -> ! {
    // SAFETY: pure kernel call; SYS_exit_group = 94 on aarch64. We
    // hardcode the NR rather than depending on `crate::nr::SYS_exit_group`
    // to keep `syscall.rs` standalone. exit_group never returns; the
    // `brk #0` after it is a defensive abort instruction in case the
    // kernel somehow does return (would never happen in practice).
    unsafe {
        const SYS_EXIT_GROUP: c_long = 94;
        let _ = syscall1(SYS_EXIT_GROUP, 0);
        core::arch::asm!("brk #0", options(noreturn));
    }
}

// Each `syscall_cp_N` mirrors `syscall_N` but routes through the asm.
// On host, falls back to `unimplemented!()` like the plain variants do.

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall_cp0(n: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: forwards to the asm; cancel flag is always 0.
    unsafe {
        __syscall_cp_asm(core::ptr::addr_of_mut!(DUMMY_CANCEL), n, 0, 0, 0, 0, 0, 0)
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp0 only runs on aarch64-linux")
    }
}

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall_cp1(n: c_long, a0: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall_cp0.
    unsafe {
        __syscall_cp_asm(core::ptr::addr_of_mut!(DUMMY_CANCEL), n, a0, 0, 0, 0, 0, 0)
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp1 only runs on aarch64-linux")
    }
}

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall_cp2(n: c_long, a0: c_long, a1: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall_cp0.
    unsafe {
        __syscall_cp_asm(core::ptr::addr_of_mut!(DUMMY_CANCEL), n, a0, a1, 0, 0, 0, 0)
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp2 only runs on aarch64-linux")
    }
}

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall_cp3(n: c_long, a0: c_long, a1: c_long, a2: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall_cp0.
    unsafe {
        __syscall_cp_asm(
            core::ptr::addr_of_mut!(DUMMY_CANCEL),
            n,
            a0,
            a1,
            a2,
            0,
            0,
            0,
        )
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp3 only runs on aarch64-linux")
    }
}

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables, clippy::too_many_arguments)]
pub unsafe fn syscall_cp4(n: c_long, a0: c_long, a1: c_long, a2: c_long, a3: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall_cp0.
    unsafe {
        __syscall_cp_asm(
            core::ptr::addr_of_mut!(DUMMY_CANCEL),
            n,
            a0,
            a1,
            a2,
            a3,
            0,
            0,
        )
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp4 only runs on aarch64-linux")
    }
}

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables, clippy::too_many_arguments)]
pub unsafe fn syscall_cp5(
    n: c_long,
    a0: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall_cp0.
    unsafe {
        __syscall_cp_asm(
            core::ptr::addr_of_mut!(DUMMY_CANCEL),
            n,
            a0,
            a1,
            a2,
            a3,
            a4,
            0,
        )
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp5 only runs on aarch64-linux")
    }
}

/// # Safety
/// See module docs.
#[inline]
#[allow(unused_variables, clippy::too_many_arguments)]
pub unsafe fn syscall_cp6(
    n: c_long,
    a0: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
    a5: c_long,
) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: see syscall_cp0.
    unsafe {
        __syscall_cp_asm(
            core::ptr::addr_of_mut!(DUMMY_CANCEL),
            n,
            a0,
            a1,
            a2,
            a3,
            a4,
            a5,
        )
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        unimplemented!("mytilus-sys::syscall_cp6 only runs on aarch64-linux")
    }
}
