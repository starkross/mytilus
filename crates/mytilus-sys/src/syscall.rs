//! AArch64 syscall stubs.
//!
//! ABI: `x8` = syscall number, `x0`..`x5` = args, return in `x0`.
//! Errors come back as `-errno` in the range `-4096..-1`; classification lives
//! in `errno_raw` so call sites stay simple.
//!
//! `__syscall_cp` (the cancellation-point variant) is intentionally NOT here —
//! it must remain handwritten assembly so the cancel handler can recognise the
//! exact PC range. See `crates/mytilus-thread/src/asm/syscall_cp.S` (TODO).

use crate::ctypes::c_long;

/// Issue a syscall with no arguments.
#[inline]
#[allow(unused_variables)]
pub unsafe fn syscall0(n: c_long) -> c_long {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    unsafe {
        let mut x0: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") n,
            lateout("x0") x0,
            options(nostack),
        );
        x0
    }
    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    {
        // Host build (macOS / x86_64-linux dev box): never executed.
        unimplemented!("mytilus-sys::syscall0 only runs on aarch64-linux")
    }
}

// TODO: syscall1..syscall6 with the same shape, plus a `__syscall_cp` FFI
// declaration that resolves to the assembly file in mytilus-thread.

/// Result classifier: kernel returns negative errno in `-4096..-1`.
#[inline]
pub fn is_err(rc: c_long) -> bool {
    (rc as u64) >= (-4096_i64 as u64)
}
