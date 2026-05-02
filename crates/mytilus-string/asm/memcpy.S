// Ported from musl 1.2.6: src/string/aarch64/memcpy.S.
// Original copyright Arm Limited 2012–2020, MIT-licensed (matches mytilus's
// MIT license).
//
// One change from upstream: `#define X Y` register-alias macros are
// translated into AArch64 GAS `.req` directives, because LLVM's integrated
// assembler doesn't run the C preprocessor and `core::arch::global_asm!`
// feeds the file directly to it. `.unreq` cleanup at the end keeps the
// aliases out of the global assembly scope so they don't collide with
// other `.S` files included via `global_asm!`.

dstin   .req x0
src     .req x1
count   .req x2
dst     .req x3
srcend  .req x4
dstend  .req x5
A_l     .req x6
A_lw    .req w6
A_h     .req x7
B_l     .req x8
B_lw    .req w8
B_h     .req x9
C_l     .req x10
C_lw    .req w10
C_h     .req x11
D_l     .req x12
D_h     .req x13
E_l     .req x14
E_h     .req x15
F_l     .req x16
F_h     .req x17
// G_l / G_h / H_l / H_h alias the same physical regs as count/dst/src/srcend
// upstream — we just use the underlying x-regs so we don't double-define.
// tmp1 aliases x14 (same as E_l; safe because the long-copy path doesn't use
// E_l).
tmp1    .req x14

/* This implementation of memcpy uses unaligned accesses and branchless
   sequences to keep the code small, simple and improve performance.

   Copies are split into 3 main cases: small copies of up to 32 bytes, medium
   copies of up to 128 bytes, and large copies.  The overhead of the overlap
   check is negligible since it is only required for large copies.

   Large copies use a software pipelined loop processing 64 bytes per iteration.
   The destination pointer is 16-byte aligned to minimize unaligned accesses.
   The loop tail is handled by always copying 64 bytes from the end.
*/

.global memcpy
.type memcpy,%function
memcpy:
	add     srcend, src, count
	add     dstend, dstin, count
	cmp     count, 128
	b.hi    .Lcopy_long
	cmp     count, 32
	b.hi    .Lcopy32_128

	/* Small copies: 0..32 bytes.  */
	cmp     count, 16
	b.lo    .Lcopy16
	ldp     A_l, A_h, [src]
	ldp     D_l, D_h, [srcend, -16]
	stp     A_l, A_h, [dstin]
	stp     D_l, D_h, [dstend, -16]
	ret

	/* Copy 8-15 bytes.  */
.Lcopy16:
	tbz     count, 3, .Lcopy8
	ldr     A_l, [src]
	ldr     A_h, [srcend, -8]
	str     A_l, [dstin]
	str     A_h, [dstend, -8]
	ret

	.p2align 3
	/* Copy 4-7 bytes.  */
.Lcopy8:
	tbz     count, 2, .Lcopy4
	ldr     A_lw, [src]
	ldr     B_lw, [srcend, -4]
	str     A_lw, [dstin]
	str     B_lw, [dstend, -4]
	ret

	/* Copy 0..3 bytes using a branchless sequence.  */
.Lcopy4:
	cbz     count, .Lcopy0
	lsr     tmp1, count, 1
	ldrb    A_lw, [src]
	ldrb    C_lw, [srcend, -1]
	ldrb    B_lw, [src, tmp1]
	strb    A_lw, [dstin]
	strb    B_lw, [dstin, tmp1]
	strb    C_lw, [dstend, -1]
.Lcopy0:
	ret

	.p2align 4
	/* Medium copies: 33..128 bytes.  */
.Lcopy32_128:
	ldp     A_l, A_h, [src]
	ldp     B_l, B_h, [src, 16]
	ldp     C_l, C_h, [srcend, -32]
	ldp     D_l, D_h, [srcend, -16]
	cmp     count, 64
	b.hi    .Lcopy128
	stp     A_l, A_h, [dstin]
	stp     B_l, B_h, [dstin, 16]
	stp     C_l, C_h, [dstend, -32]
	stp     D_l, D_h, [dstend, -16]
	ret

	.p2align 4
	/* Copy 65..128 bytes.  */
.Lcopy128:
	ldp     E_l, E_h, [src, 32]
	ldp     F_l, F_h, [src, 48]
	cmp     count, 96
	b.ls    .Lcopy96
	// Upstream uses G_l/G_h/H_l/H_h here (= count/dst/src/srcend regs);
	// we just use the underlying registers directly.
	ldp     count, dst, [srcend, -64]
	ldp     src, srcend, [srcend, -48]
	stp     count, dst, [dstend, -64]
	stp     src, srcend, [dstend, -48]
.Lcopy96:
	stp     A_l, A_h, [dstin]
	stp     B_l, B_h, [dstin, 16]
	stp     E_l, E_h, [dstin, 32]
	stp     F_l, F_h, [dstin, 48]
	stp     C_l, C_h, [dstend, -32]
	stp     D_l, D_h, [dstend, -16]
	ret

	.p2align 4
	/* Copy more than 128 bytes.  */
.Lcopy_long:

	/* Copy 16 bytes and then align dst to 16-byte alignment.  */

	ldp     D_l, D_h, [src]
	and     tmp1, dstin, 15
	bic     dst, dstin, 15
	sub     src, src, tmp1
	add     count, count, tmp1      /* Count is now 16 too large.  */
	ldp     A_l, A_h, [src, 16]
	stp     D_l, D_h, [dstin]
	ldp     B_l, B_h, [src, 32]
	ldp     C_l, C_h, [src, 48]
	ldp     D_l, D_h, [src, 64]!
	subs    count, count, 128 + 16  /* Test and readjust count.  */
	b.ls    .Lcopy64_from_end

.Lloop64:
	stp     A_l, A_h, [dst, 16]
	ldp     A_l, A_h, [src, 16]
	stp     B_l, B_h, [dst, 32]
	ldp     B_l, B_h, [src, 32]
	stp     C_l, C_h, [dst, 48]
	ldp     C_l, C_h, [src, 48]
	stp     D_l, D_h, [dst, 64]!
	ldp     D_l, D_h, [src, 64]!
	subs    count, count, 64
	b.hi    .Lloop64

	/* Write the last iteration and copy 64 bytes from the end.  */
.Lcopy64_from_end:
	ldp     E_l, E_h, [srcend, -64]
	stp     A_l, A_h, [dst, 16]
	ldp     A_l, A_h, [srcend, -48]
	stp     B_l, B_h, [dst, 32]
	ldp     B_l, B_h, [srcend, -32]
	stp     C_l, C_h, [dst, 48]
	ldp     C_l, C_h, [srcend, -16]
	stp     D_l, D_h, [dst, 64]
	stp     E_l, E_h, [dstend, -64]
	stp     A_l, A_h, [dstend, -48]
	stp     B_l, B_h, [dstend, -32]
	stp     C_l, C_h, [dstend, -16]
	ret

.size memcpy,.-memcpy

// Drop the `.req` aliases so they don't collide with subsequent global_asm!
// invocations (e.g. memset.S).
.unreq dstin
.unreq src
.unreq count
.unreq dst
.unreq srcend
.unreq dstend
.unreq A_l
.unreq A_lw
.unreq A_h
.unreq B_l
.unreq B_lw
.unreq B_h
.unreq C_l
.unreq C_lw
.unreq C_h
.unreq D_l
.unreq D_h
.unreq E_l
.unreq E_h
.unreq F_l
.unreq F_h
.unreq tmp1
