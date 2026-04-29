//! `mytilus-mman` — `mmap` family.
//!
//! Mirrors `src/mman/` upstream (Phase 1: 12 syscalls, no `shm_*`).
//! `shm_open`/`shm_unlink` are deferred — they're userspace wrappers around
//! `open`/`unlink` on `/dev/shm/`, so they need `mytilus-fcntl::open`.
//!
//! Every public function follows the same shape:
//!   1. Optional caller-side validation (e.g. `mmap` rejects `len >= PTRDIFF_MAX`).
//!   2. `syscallN(SYS_X, args...)` via `mytilus-sys`.
//!   3. `mytilus_sys::syscall::ret(r)` to set `errno` on `-errno` and return `-1`.
//!
//! TODO(thread): upstream `mmap`/`munmap`/`mremap` call `__vm_wait()` (a weak
//! symbol that's a no-op until `mytilus-thread` provides one) when the
//! `MAP_FIXED` / `MREMAP_FIXED` flag is set, to drain concurrent mmap-ers.
//! We just don't call it — single-threaded behavior is unchanged. Hook this
//! up when `mytilus-thread` lands.
//!
//! TODO(auxv/page-size): upstream `mprotect` rounds `addr` down and
//! `addr+len` up to `PAGE_SIZE`. We pass through unmodified — the kernel
//! returns `EINVAL` on misaligned `addr`, which is the documented contract.
//! Add musl's lenient rounding once `mytilus-startup` reads `AT_PAGESZ` from
//! the auxv.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

// Force-link mytilus-errno so its `__errno_location` symbol is in the final
// binary; mytilus-sys::syscall::ret() resolves to it via extern "C". Without
// this `extern crate`, rustc would drop the rlib because we don't name any
// item from it directly.
extern crate mytilus_errno;

use mytilus_sys::ctypes::{c_int, c_long, c_uchar, c_void, off_t, size_t};
use mytilus_sys::errno_raw::{EINVAL, ENOMEM, EPERM};
use mytilus_sys::nr::*;
use mytilus_sys::syscall::{ret, syscall1, syscall2, syscall3, syscall5, syscall6};

// ---------------------------------------------------------------------------
// Constants — kernel ABI for AArch64 Linux. Values must match `bits/mman.h`
// (which itself matches `<linux/asm-generic/mman.h>`).
// ---------------------------------------------------------------------------

pub const MAP_FAILED: *mut c_void = -1isize as *mut c_void;

// MAP_* flags
pub const MAP_SHARED: c_int = 0x01;
pub const MAP_PRIVATE: c_int = 0x02;
pub const MAP_SHARED_VALIDATE: c_int = 0x03;
pub const MAP_TYPE: c_int = 0x0f;
pub const MAP_FIXED: c_int = 0x10;
pub const MAP_ANON: c_int = 0x20;
pub const MAP_ANONYMOUS: c_int = MAP_ANON;
pub const MAP_NORESERVE: c_int = 0x4000;
pub const MAP_GROWSDOWN: c_int = 0x0100;
pub const MAP_DENYWRITE: c_int = 0x0800;
pub const MAP_EXECUTABLE: c_int = 0x1000;
pub const MAP_LOCKED: c_int = 0x2000;
pub const MAP_POPULATE: c_int = 0x8000;
pub const MAP_NONBLOCK: c_int = 0x10000;
pub const MAP_STACK: c_int = 0x20000;
pub const MAP_HUGETLB: c_int = 0x40000;
pub const MAP_SYNC: c_int = 0x80000;
pub const MAP_FIXED_NOREPLACE: c_int = 0x100000;
pub const MAP_FILE: c_int = 0;

// PROT_* flags
pub const PROT_NONE: c_int = 0;
pub const PROT_READ: c_int = 1;
pub const PROT_WRITE: c_int = 2;
pub const PROT_EXEC: c_int = 4;
pub const PROT_GROWSDOWN: c_int = 0x0100_0000;
pub const PROT_GROWSUP: c_int = 0x0200_0000;

// MS_* (msync)
pub const MS_ASYNC: c_int = 1;
pub const MS_INVALIDATE: c_int = 2;
pub const MS_SYNC: c_int = 4;

// MCL_* (mlockall)
pub const MCL_CURRENT: c_int = 1;
pub const MCL_FUTURE: c_int = 2;
pub const MCL_ONFAULT: c_int = 4;

// POSIX_MADV_*
pub const POSIX_MADV_NORMAL: c_int = 0;
pub const POSIX_MADV_RANDOM: c_int = 1;
pub const POSIX_MADV_SEQUENTIAL: c_int = 2;
pub const POSIX_MADV_WILLNEED: c_int = 3;
pub const POSIX_MADV_DONTNEED: c_int = 4;

// MADV_* (Linux extensions)
pub const MADV_NORMAL: c_int = 0;
pub const MADV_RANDOM: c_int = 1;
pub const MADV_SEQUENTIAL: c_int = 2;
pub const MADV_WILLNEED: c_int = 3;
pub const MADV_DONTNEED: c_int = 4;
pub const MADV_FREE: c_int = 8;
pub const MADV_REMOVE: c_int = 9;
pub const MADV_DONTFORK: c_int = 10;
pub const MADV_DOFORK: c_int = 11;
pub const MADV_HUGEPAGE: c_int = 14;
pub const MADV_NOHUGEPAGE: c_int = 15;
pub const MADV_DONTDUMP: c_int = 16;
pub const MADV_DODUMP: c_int = 17;

// MREMAP_*
pub const MREMAP_MAYMOVE: c_int = 1;
pub const MREMAP_FIXED: c_int = 2;
pub const MREMAP_DONTUNMAP: c_int = 4;

// PTRDIFF_MAX — used in mmap/mremap length validation. On LP64 aarch64,
// ptrdiff_t is i64, so PTRDIFF_MAX is i64::MAX.
const PTRDIFF_MAX: size_t = isize::MAX as size_t;

// Page size assumed for the off-mask validation in `mmap`. PLAN.md commits
// to 4K-page aarch64 kernels for the single target; if we ever care about
// 16K/64K kernel pages, this needs to come from auxv (`AT_PAGESZ`).
const PAGE_SIZE: u64 = 4096;
const OFF_MASK: u64 = PAGE_SIZE - 1;

// ---------------------------------------------------------------------------
// mmap / munmap / mprotect / mremap
// ---------------------------------------------------------------------------

/// `void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset)`
///
/// # Safety
/// `addr` may be NULL or a valid mapping hint. `fd` must be a valid file
/// descriptor for file-backed mappings (ignored for `MAP_ANON`). Other args
/// follow the Linux `mmap(2)` contract.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn mmap(
    addr: *mut c_void,
    length: size_t,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    offset: off_t,
) -> *mut c_void {
    // Caller-side validation matching upstream src/mman/mmap.c.
    if (offset as u64) & OFF_MASK != 0 {
        // SAFETY: setting errno through the linked __errno_location.
        unsafe {
            set_errno(EINVAL);
        }
        return MAP_FAILED;
    }
    if length >= PTRDIFF_MAX {
        // SAFETY: see above.
        unsafe {
            set_errno(ENOMEM);
        }
        return MAP_FAILED;
    }
    // (TODO(thread): __vm_wait() on MAP_FIXED — see module-level note.)

    // SAFETY: kernel mmap takes byte-offset directly on aarch64-LP64 (no
    // SYS_mmap2 page-scaling required).
    let r = unsafe {
        syscall6(
            SYS_mmap,
            addr as c_long,
            length as c_long,
            prot as c_long,
            flags as c_long,
            fd as c_long,
            offset as c_long,
        )
    };

    // Upstream "fixup": kernel returns EPERM for some anonymous-without-FIXED
    // mappings where the caller really meant ENOMEM. Mirror it.
    let r = if r == -(EPERM as c_long)
        && addr.is_null()
        && (flags & MAP_ANON) != 0
        && (flags & MAP_FIXED) == 0
    {
        -(ENOMEM as c_long)
    } else {
        r
    };

    // SAFETY: ret() reads/writes the per-thread errno slot via __errno_location.
    unsafe { ret(r) as *mut c_void }
}

/// `int munmap(void *addr, size_t length)`
///
/// # Safety
/// `addr` must be the result of a previous `mmap` (or page-aligned and
/// within a mapped range), and `length` must be a multiple of the page size.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn munmap(addr: *mut c_void, length: size_t) -> c_int {
    // (TODO(thread): unconditional __vm_wait() upstream — see module note.)
    // SAFETY: kernel does the heavy lifting; we just forward.
    let r = unsafe { syscall2(SYS_munmap, addr as c_long, length as c_long) };
    // SAFETY: ret() classifies the return.
    unsafe { ret(r) as c_int }
}

/// `int mprotect(void *addr, size_t length, int prot)`
///
/// # Safety
/// `addr` must be page-aligned (we don't round). `length` should cover whole
/// pages (rounded up by the kernel).
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn mprotect(addr: *mut c_void, length: size_t, prot: c_int) -> c_int {
    // SAFETY: forwards to kernel; see TODO(auxv/page-size) at module level —
    // we do not round addr/length to PAGE_SIZE the way upstream does.
    let r = unsafe {
        syscall3(
            SYS_mprotect,
            addr as c_long,
            length as c_long,
            prot as c_long,
        )
    };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `void *mremap(void *old_addr, size_t old_len, size_t new_len, int flags, void *new_addr)`
///
/// Note: upstream's C signature is variadic (`mremap(..., flags, ...)` with
/// `new_addr` only consumed when `flags & MREMAP_FIXED`). We expose `new_addr`
/// as a regular fixed argument because varargs add no value here — callers
/// who don't pass `MREMAP_FIXED` simply pass `core::ptr::null_mut()`.
///
/// # Safety
/// `old_addr` must be a previously-mapped region of length `old_len`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn mremap(
    old_addr: *mut c_void,
    old_len: size_t,
    new_len: size_t,
    flags: c_int,
    new_addr: *mut c_void,
) -> *mut c_void {
    if new_len >= PTRDIFF_MAX {
        // SAFETY: errno write only.
        unsafe {
            set_errno(ENOMEM);
        }
        return MAP_FAILED;
    }
    // (TODO(thread): __vm_wait() on MREMAP_FIXED — see module note.)

    // SAFETY: forwards to the kernel.
    let r = unsafe {
        syscall5(
            SYS_mremap,
            old_addr as c_long,
            old_len as c_long,
            new_len as c_long,
            flags as c_long,
            new_addr as c_long,
        )
    };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as *mut c_void }
}

// ---------------------------------------------------------------------------
// msync / madvise / posix_madvise / mincore
// ---------------------------------------------------------------------------

/// `int msync(void *addr, size_t length, int flags)`
///
/// # Safety
/// `addr` must point to a mapped region of length `length`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn msync(addr: *mut c_void, length: size_t, flags: c_int) -> c_int {
    // Note: upstream uses `syscall_cp` (cancellation-point variant) here.
    // We call the regular svc; cancellation isn't wired up yet and msync
    // without it is functionally correct for non-cancellable callers.
    // TODO(thread): switch to __syscall_cp once mytilus-thread's asm lands.
    // SAFETY: forwards to the kernel.
    let r = unsafe { syscall3(SYS_msync, addr as c_long, length as c_long, flags as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int madvise(void *addr, size_t length, int advice)`
///
/// # Safety
/// `addr` must point to a mapped region of length `length`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn madvise(addr: *mut c_void, length: size_t, advice: c_int) -> c_int {
    // SAFETY: forwards to the kernel.
    let r = unsafe {
        syscall3(
            SYS_madvise,
            addr as c_long,
            length as c_long,
            advice as c_long,
        )
    };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int posix_madvise(void *addr, size_t length, int advice)` —
/// returns `errno` directly (NOT -1 + errno) per POSIX. `MADV_DONTNEED` is
/// always reported as success because Linux's destructive madvise differs
/// from POSIX semantics; upstream takes the same shortcut.
///
/// # Safety
/// See `madvise`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn posix_madvise(addr: *mut c_void, length: size_t, advice: c_int) -> c_int {
    if advice == MADV_DONTNEED {
        return 0;
    }
    // SAFETY: forwards to the kernel; we negate the negative-errno return to
    // match POSIX (positive errno on failure, 0 on success), bypassing the
    // standard `ret()` helper.
    let r = unsafe {
        syscall3(
            SYS_madvise,
            addr as c_long,
            length as c_long,
            advice as c_long,
        )
    };
    -r as c_int
}

/// `int mincore(void *addr, size_t length, unsigned char *vec)`
///
/// # Safety
/// `addr` must be page-aligned and a mapped range; `vec` must be writable
/// for `(length + page_size - 1) / page_size` bytes.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn mincore(addr: *mut c_void, length: size_t, vec: *mut c_uchar) -> c_int {
    // SAFETY: forwards to the kernel.
    let r = unsafe { syscall3(SYS_mincore, addr as c_long, length as c_long, vec as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// mlock / munlock / mlockall / munlockall
// ---------------------------------------------------------------------------

/// `int mlock(const void *addr, size_t length)`
///
/// # Safety
/// `addr` must point to readable memory of length `length`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn mlock(addr: *const c_void, length: size_t) -> c_int {
    // SAFETY: forwards to the kernel. AArch64 has SYS_mlock; the SYS_mlock2
    // fallback the upstream C uses isn't needed here.
    let r = unsafe { syscall2(SYS_mlock, addr as c_long, length as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int munlock(const void *addr, size_t length)`
///
/// # Safety
/// See `mlock`.
#[cfg_attr(not(test), no_mangle)]
pub unsafe extern "C" fn munlock(addr: *const c_void, length: size_t) -> c_int {
    // SAFETY: forwards to the kernel.
    let r = unsafe { syscall2(SYS_munlock, addr as c_long, length as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int mlockall(int flags)`
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn mlockall(flags: c_int) -> c_int {
    // SAFETY: no caller-supplied pointers; pure kernel call.
    let r = unsafe { syscall1(SYS_mlockall, flags as c_long) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

/// `int munlockall(void)`
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn munlockall() -> c_int {
    // SAFETY: no args.
    let r = unsafe { mytilus_sys::syscall::syscall0(SYS_munlockall) };
    // SAFETY: ret() classifies.
    unsafe { ret(r) as c_int }
}

// ---------------------------------------------------------------------------
// errno helper used by mmap/mremap (which write errno without going through
// `ret()` because they need to return MAP_FAILED, not -1)
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn __errno_location() -> *mut c_int;
}

/// # Safety
/// `__errno_location` must be linked in (`mytilus-errno`).
#[inline]
unsafe fn set_errno(e: c_int) {
    // SAFETY: errno slot is per-thread and writable per our contract.
    unsafe {
        *__errno_location() = e;
    }
}

#[cfg(test)]
mod tests {
    //! Behavioral tests can't run on host: every public function ultimately
    //! invokes `syscallN`, which is `unimplemented!()` on macOS / x86_64
    //! Linux dev hosts. Real coverage runs under `qemu-aarch64` once
    //! `task test:qemu` is wired (see Taskfile TODO).
    //!
    //! What we *can* check on host is that the publicly-exported constants
    //! match the Linux ABI values. If any of these drift we get a clean
    //! compile error instead of a silent runtime fault on the target.

    use super::*;

    #[test]
    fn map_flags_match_linux_abi() {
        assert_eq!(MAP_SHARED, 0x01);
        assert_eq!(MAP_PRIVATE, 0x02);
        assert_eq!(MAP_FIXED, 0x10);
        assert_eq!(MAP_ANON, 0x20);
        assert_eq!(MAP_ANONYMOUS, MAP_ANON);
        assert_eq!(MAP_FAILED as isize, -1);
    }

    #[test]
    fn prot_flags_match_linux_abi() {
        assert_eq!(PROT_NONE, 0);
        assert_eq!(PROT_READ, 1);
        assert_eq!(PROT_WRITE, 2);
        assert_eq!(PROT_EXEC, 4);
    }

    #[test]
    fn msync_flags_match_linux_abi() {
        assert_eq!(MS_ASYNC, 1);
        assert_eq!(MS_INVALIDATE, 2);
        assert_eq!(MS_SYNC, 4);
    }

    #[test]
    fn madv_constants_match_linux_abi() {
        assert_eq!(MADV_NORMAL, 0);
        assert_eq!(MADV_DONTNEED, 4);
        assert_eq!(MADV_FREE, 8);
        // POSIX values overlap with the Linux ones for the common cases.
        assert_eq!(POSIX_MADV_NORMAL, 0);
        assert_eq!(POSIX_MADV_DONTNEED, 4);
    }

    #[test]
    fn mremap_constants_match_linux_abi() {
        assert_eq!(MREMAP_MAYMOVE, 1);
        assert_eq!(MREMAP_FIXED, 2);
    }

    #[test]
    fn syscall_numbers_match_aarch64_abi() {
        // From arch/aarch64/bits/syscall.h.in upstream.
        assert_eq!(SYS_mmap, 222);
        assert_eq!(SYS_munmap, 215);
        assert_eq!(SYS_mprotect, 226);
        assert_eq!(SYS_mremap, 216);
        assert_eq!(SYS_msync, 227);
        assert_eq!(SYS_madvise, 233);
        assert_eq!(SYS_mincore, 232);
        assert_eq!(SYS_mlock, 228);
        assert_eq!(SYS_munlock, 229);
        assert_eq!(SYS_mlockall, 230);
        assert_eq!(SYS_munlockall, 231);
    }
}
