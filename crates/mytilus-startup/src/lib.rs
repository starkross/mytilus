//! `mytilus-startup` — C runtime entry points and `<setjmp.h>`.
//!
//! Phase 1 ports just **setjmp / longjmp**. crt1 / crti / crtn / Scrt1 /
//! rcrt1 / `__libc_start_main` come later and need much more
//! infrastructure (auxv reader, env array, dynamic linker handoff).
//!
//! ## Build infrastructure note
//!
//! The asm is brought in via [`core::arch::global_asm!`] + [`include_str!`]
//! rather than a build script + `cc` crate. This is the lightest possible
//! option: no external assembler invocation, no crates.io build-deps
//! (which our workspace policy forbids), and Cargo's normal change
//! detection just works because the .s files are direct includes. Every
//! future `.S` port (`memcpy.S`, `memset.S`, `syscall_cp.S`, `dlstart`,
//! `clone.s`) can use the same pattern — they're all aarch64 ELF asm.
//!
//! The `global_asm!` invocation is `cfg`-gated to aarch64-linux because
//! the assembly contains aarch64-only mnemonics (`stp`/`ldp`/`csinc`/
//! `svc` etc.) that LLVM's integrated assembler would reject on host
//! targets like x86_64 or aarch64-apple-darwin.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

// `c_int` is only used by the extern decls below, which are
// aarch64-linux-only. Pull it in there to keep host builds warning-free.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
use mytilus_sys::ctypes::c_int;
use mytilus_sys::ctypes::c_ulong;

// ---------------------------------------------------------------------------
// jmp_buf shape
// ---------------------------------------------------------------------------
//
// On AArch64 LP64, `__jmp_buf` is `unsigned long [22]` (matches
// `arch/aarch64/bits/setjmp.h` upstream): 22 × 8 = 176 bytes covering
// x19/x20, x21/x22, x23/x24, x25/x26, x27/x28, x29/x30, sp, d8/d9,
// d10/d11, d12/d13, d14/d15.
//
// `jmp_buf` itself is a 1-element array of struct `__jmp_buf_tag` (a C
// quirk that lets `jmp_buf` decay to a pointer when passed by name). The
// struct also carries `__fl` (signal-mask save flag) and `__ss` (the
// signal mask itself) for `sigsetjmp`/`siglongjmp` — plain `setjmp` /
// `longjmp` don't touch those fields.

/// 22-element `unsigned long` array — the register-save area.
pub type __jmp_buf = [c_ulong; 22];

#[repr(C)]
#[derive(Copy, Clone)]
pub struct __jmp_buf_tag {
    pub __jb: __jmp_buf,
    pub __fl: c_ulong,
    pub __ss: [c_ulong; 16],
}

/// `typedef struct __jmp_buf_tag jmp_buf[1];` — the pointer-decay form
/// callers see.
pub type jmp_buf = [__jmp_buf_tag; 1];

/// `sigjmp_buf` is structurally identical to `jmp_buf`; only
/// `sigsetjmp`/`siglongjmp` ever consult the `__fl` / `__ss` fields.
pub type sigjmp_buf = jmp_buf;

// ---------------------------------------------------------------------------
// Assembly inclusion + symbol declarations
// ---------------------------------------------------------------------------

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
core::arch::global_asm!(include_str!("../asm/setjmp.s"));

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
core::arch::global_asm!(include_str!("../asm/longjmp.s"));

// Declarations let Rust callers reach the asm-defined symbols.  On
// non-aarch64-linux targets the asm isn't included, so we don't declare
// the externs either — any caller will see a clean "function not found"
// error rather than an obscure link failure.

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
unsafe extern "C" {
    /// `int setjmp(jmp_buf env)` — saves regs, returns 0 on initial call.
    /// Marked `returns_twice` in C; that attribute has no Rust equivalent
    /// but doesn't matter at the FFI boundary because callers see only
    /// the C-ABI signature.
    pub fn setjmp(env: *mut __jmp_buf_tag) -> c_int;

    /// `int _setjmp(jmp_buf env)` — POSIX-spelled alias for `setjmp`.
    pub fn _setjmp(env: *mut __jmp_buf_tag) -> c_int;

    /// `int __setjmp(jmp_buf env)` — glibc-internal alias.
    pub fn __setjmp(env: *mut __jmp_buf_tag) -> c_int;

    /// `void longjmp(jmp_buf env, int val) -> !` — restores regs, returns
    /// `val` (or 1 if `val==0`) at the matching `setjmp` call site.
    pub fn longjmp(env: *mut __jmp_buf_tag, val: c_int) -> !;

    /// `void _longjmp(jmp_buf env, int val) -> !`.
    pub fn _longjmp(env: *mut __jmp_buf_tag, val: c_int) -> !;
}

#[cfg(test)]
mod tests {
    //! Behavior tests can't run on host: the asm is cfg-gated to
    //! aarch64-linux, so `setjmp`/`longjmp` aren't even declared on
    //! macOS / x86_64-linux. We assert the FFI struct layouts — drift
    //! here silently corrupts every consumer (in particular the eventual
    //! `<setjmp.h>` C header generation).

    use core::mem::{align_of, offset_of, size_of};

    use super::*;

    #[test]
    fn jmp_buf_layout_matches_aarch64_lp64() {
        // `__jmp_buf` is 22 × u64 = 176 bytes, 8-byte aligned. The
        // setjmp.s prologue stores up to offset 176 (last `stp d14, d15,
        // [x0, #160]` = 16 bytes ending at 176).
        assert_eq!(size_of::<__jmp_buf>(), 176);
        assert_eq!(align_of::<__jmp_buf>(), 8);
    }

    #[test]
    fn jmp_buf_tag_layout_matches_lp64_abi() {
        // struct __jmp_buf_tag = __jb (176) + __fl (8) + __ss (16×8=128)
        // = 312 bytes, 8-byte aligned. Field offsets:
        //   __jb at 0, __fl at 176, __ss at 184.
        assert_eq!(size_of::<__jmp_buf_tag>(), 312);
        assert_eq!(align_of::<__jmp_buf_tag>(), 8);
        assert_eq!(offset_of!(__jmp_buf_tag, __jb), 0);
        assert_eq!(offset_of!(__jmp_buf_tag, __fl), 176);
        assert_eq!(offset_of!(__jmp_buf_tag, __ss), 184);
    }

    #[test]
    fn jmp_buf_is_array_of_one_tag() {
        // `typedef struct __jmp_buf_tag jmp_buf[1];` — same size as one
        // tag, callers pass a pointer-decayed `*mut __jmp_buf_tag`.
        assert_eq!(size_of::<jmp_buf>(), size_of::<__jmp_buf_tag>());
        assert_eq!(size_of::<sigjmp_buf>(), size_of::<__jmp_buf_tag>());
    }

    #[test]
    fn save_offsets_match_setjmp_s() {
        // Spot-check that the slot-index → byte-offset arithmetic that
        // the .s file uses is what we expect for `c_ulong` (8 B on LP64).
        // Slot indices in `__jmp_buf`:
        //   slot 2  → offset 16  (x21/x22)
        //   slot 13 → offset 104 (sp)
        //   slot 14 → offset 112 (d8/d9)
        //   slot 20 → offset 160 (d14/d15)
        //   slot 21 → offset 168 (second half of d14/d15)
        const SLOT: usize = core::mem::size_of::<c_ulong>();
        assert_eq!(SLOT, 8);
        assert_eq!(2 * SLOT, 16);
        assert_eq!(13 * SLOT, 104);
        assert_eq!(14 * SLOT, 112);
        assert_eq!(20 * SLOT, 160);
        assert_eq!(21 * SLOT, 168);
    }
}
