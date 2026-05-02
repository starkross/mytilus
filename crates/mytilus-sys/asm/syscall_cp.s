// Ported from musl 1.2.6: src/thread/aarch64/syscall_cp.s.
// MIT licensed (matches mytilus's license).
//
// Skeleton: the cancel-flag check + svc + return are exactly upstream's;
// `__cancel` is a stub provided in Rust (unreachable until pthread_cancel
// lands, since our caller passes &DUMMY_CANCEL which is permanently 0).
//
// Eventual home is `mytilus-thread`; we keep it in `mytilus-sys` for now
// so the `syscall_cp_N` Rust wrappers can sit alongside `syscall_N`.
//
// Signature:  __syscall_cp_asm(&self->cancel, nr, u, v, w, x, y, z)
//                              x0             x1  x2 x3 x4 x5 x6 x7
// Kernel ABI: svc 0 with nr in x8, args in x0..x5, return in x0.
//
// `__cp_begin` and `__cp_end` mark the PC range a future cancel-handler
// uses to recognise "I caught a SIGCANCEL while this thread was mid-syscall".
// They're hidden so they don't pollute the dynamic symbol table.

.global __cp_begin
.hidden __cp_begin
.global __cp_end
.hidden __cp_end
.global __cp_cancel
.hidden __cp_cancel
.hidden __cancel
.global __syscall_cp_asm
.hidden __syscall_cp_asm
.type __syscall_cp_asm,%function
__syscall_cp_asm:
__cp_begin:
	ldr w0,[x0]
	cbnz w0,__cp_cancel
	mov x8,x1
	mov x0,x2
	mov x1,x3
	mov x2,x4
	mov x3,x5
	mov x4,x6
	mov x5,x7
	svc 0
__cp_end:
	ret
__cp_cancel:
	b __cancel
