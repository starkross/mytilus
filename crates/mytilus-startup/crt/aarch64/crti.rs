#![no_std]
#![no_main]

use core::panic::PanicInfo;

core::arch::global_asm!(
    r#"
    .section .init
    .global _init
    .type _init,%function
_init:
    stp x29, x30, [sp, #-16]!
    mov x29, sp

    .section .fini
    .global _fini
    .type _fini,%function
_fini:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
"#
);

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
