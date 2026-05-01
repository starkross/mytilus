// Ported verbatim from musl 1.2.6: src/setjmp/aarch64/setjmp.s.
// MIT licensed (matches mytilus's license).
//
// Saves the AAPCS64 callee-saved register set to a `__jmp_buf` so that a
// later `longjmp` can resume execution at the call site. Layout matches
// `arch/aarch64/bits/setjmp.h` upstream: 22 × u64 = 176 bytes, in the order
// x19/x20, x21/x22, x23/x24, x25/x26, x27/x28, x29/x30, sp, d8/d9, d10/d11,
// d12/d13, d14/d15.
//
// Three exported names (canonical + POSIX + glibc-internal):
//   setjmp, _setjmp, __setjmp — all alias the same body.

.global __setjmp
.global _setjmp
.global setjmp
.type __setjmp,@function
.type _setjmp,@function
.type setjmp,@function
__setjmp:
_setjmp:
setjmp:
	// IHI0055B_aapcs64.pdf 5.1.1, 5.1.2 callee saved registers
	stp x19, x20, [x0,#0]
	stp x21, x22, [x0,#16]
	stp x23, x24, [x0,#32]
	stp x25, x26, [x0,#48]
	stp x27, x28, [x0,#64]
	stp x29, x30, [x0,#80]
	mov x2, sp
	str x2, [x0,#104]
	stp  d8,  d9, [x0,#112]
	stp d10, d11, [x0,#128]
	stp d12, d13, [x0,#144]
	stp d14, d15, [x0,#160]
	mov x0, #0
	ret
