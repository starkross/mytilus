#![no_std]
#![no_main]

use core::panic::PanicInfo;

core::arch::global_asm!(
    r#"
    .section .init
    ldp x29, x30, [sp], #16
    ret

    .section .fini
    ldp x29, x30, [sp], #16
    ret
"#
);

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
