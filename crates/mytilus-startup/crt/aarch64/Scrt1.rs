#![no_std]
#![no_main]

#[path = "../start.rs"]
mod start;

core::arch::global_asm!(
    r#"
    .text
    .global _start
    .type _start,%function
_start:
    mov x29, #0
    mov x30, #0
    mov x0, sp
    .weak _DYNAMIC
    .hidden _DYNAMIC
    adrp x1, _DYNAMIC
    add x1, x1, #:lo12:_DYNAMIC
    and sp, x0, #-16
    bl _start_c
    .size _start, . - _start
"#
);
