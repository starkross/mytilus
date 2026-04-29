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
//! `__syscall_cp` (the cancellation-point variant) is intentionally NOT here —
//! it must remain handwritten assembly so the cancel handler can recognise the
//! exact PC range. See `crates/mytilus-thread/src/asm/syscall_cp.S` (TODO).
//!
//! Host fallback: when built for any non-aarch64-linux target (including the
//! macOS / x86_64-linux dev hosts we use for `cargo check` / `cargo test`),
//! every `syscallN` panics with `unimplemented!()`. That keeps the workspace
//! buildable on the host while making accidental host execution loud. Real
//! syscall coverage runs under `qemu-aarch64` (TODO: wire `task test:qemu`).

use crate::ctypes::c_long;

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
