#![allow(non_camel_case_types)]

use core::panic::PanicInfo;

type c_int = i32;
type Main = unsafe extern "C" fn(c_int, *mut *mut u8, *mut *mut u8) -> c_int;

unsafe extern "C" {
    fn main(argc: c_int, argv: *mut *mut u8, envp: *mut *mut u8) -> c_int;

    fn __libc_start_main(
        main: Main,
        argc: c_int,
        argv: *mut *mut u8,
        init: usize,
        fini: usize,
        ldso_fini: usize,
    ) -> !;
}

#[no_mangle]
pub unsafe extern "C" fn _start_c(stack: *mut usize, _dynamic: *mut usize) -> ! {
    let argc = unsafe { *stack } as c_int;
    let argv = unsafe { stack.add(1).cast::<*mut u8>() };

    unsafe { __libc_start_main(main, argc, argv, 0, 0, 0) }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
