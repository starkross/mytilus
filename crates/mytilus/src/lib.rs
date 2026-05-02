//! `mytilus` — Umbrella crate. Re-exports every public symbol; built as libc.so / libc.a.
//!
//! Part of mytilus. Target: aarch64-unknown-linux, 64-bit only.
//!
//! Status: skeleton — no public symbols implemented yet.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

#[cfg(not(test))]
use core::panic::PanicInfo;

/// Abort the entire process on an internal libc panic.
///
/// Panics must never unwind across the C ABI. The eventual implementation can
/// write a diagnostic to stderr before exiting; for now we terminate directly.
#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // SAFETY: Linux AArch64 syscall ABI: x8 holds the syscall number and x0
    // holds the first argument. SYS_exit_group is 94. If the kernel were ever
    // to return, `brk #0` traps immediately.
    unsafe {
        core::arch::asm!(
            "svc #0",
            "brk #0",
            in("x8") 94usize,
            in("x0") 127usize,
            options(noreturn),
        );
    }

    #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
    loop {
        core::hint::spin_loop();
    }
}
