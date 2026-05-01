//! `qsort`, `qsort_r`, `bsearch`.
//!
//! Mirrors `src/stdlib/{qsort,qsort_nr,bsearch}.c` upstream — but we
//! deliberately use **heapsort** rather than upstream's smoothsort. Reasons:
//!
//! - Heapsort is ~50 LOC of straightforward code; smoothsort is ~230 LOC of
//!   intricate adaptive-heap state-machine math (tree of fused Leonardo
//!   heaps). Correctness-first wins for now.
//! - Both are O(n log n) worst case, in-place, allocation-free. The
//!   smoothsort win is its near-O(n) behavior on already-sorted input,
//!   which we don't need until something benchmarks slow.
//! - `TODO(perf)`: port the upstream smoothsort verbatim once we have a
//!   bench harness. The C version is in `src/stdlib/qsort.c`; the public
//!   ABI here doesn't change.
//!
//! `bsearch` is the upstream impl verbatim — straightforward halving loop.

use mytilus_sys::ctypes::{c_int, c_void, size_t};

// ---------------------------------------------------------------------------
// Comparator types
// ---------------------------------------------------------------------------

/// 2-arg comparator used by `qsort` and `bsearch`.
///
/// Returns negative if `a < b`, 0 if equal, positive if `a > b`.
pub type CmpFn = extern "C" fn(*const c_void, *const c_void) -> c_int;

/// 3-arg comparator used by `qsort_r`. The third arg is the user-supplied
/// context pointer threaded through unchanged.
pub type CmpFnR = extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int;

// ---------------------------------------------------------------------------
// Internal: byte-level helpers
// ---------------------------------------------------------------------------

/// Pointer to the i'th element (`base + i*width`).
#[inline]
unsafe fn elt(base: *mut u8, width: size_t, i: size_t) -> *mut u8 {
    // SAFETY: caller guarantees i*width is in-bounds.
    unsafe { base.add(i * width) }
}

/// Swap two `width`-byte regions byte-by-byte. No allocator dep, no
/// alignment assumptions. `TODO(perf)`: word-stride / chunked-buffer swap
/// is ~10-100x faster on large widths but adds complexity; revisit when
/// profiling shows it matters.
///
/// # Safety
/// `a` and `b` must each be valid for `width` bytes; they must not alias
/// (qsort's heapsort never swaps an element with itself).
#[inline]
unsafe fn swap_bytes(a: *mut u8, b: *mut u8, width: size_t) {
    // SAFETY: caller asserts both ranges are valid and disjoint.
    unsafe {
        let mut i: size_t = 0;
        while i < width {
            let t = *a.add(i);
            *a.add(i) = *b.add(i);
            *b.add(i) = t;
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Heapsort
// ---------------------------------------------------------------------------

/// Sift down a max-heap rooted at index `i` over the first `n` elements.
///
/// # Safety
/// `base` valid for `n*width` bytes; `cmp(ctx)` must be sound to call.
unsafe fn sift_down(
    base: *mut u8,
    n: size_t,
    width: size_t,
    mut i: size_t,
    cmp: CmpFnR,
    ctx: *mut c_void,
) {
    loop {
        let mut child = 2 * i + 1;
        if child >= n {
            return;
        }
        // SAFETY: indexes are heap-bounded by checks above.
        unsafe {
            // Pick the larger of the two children.
            if child + 1 < n
                && cmp(
                    elt(base, width, child) as *const c_void,
                    elt(base, width, child + 1) as *const c_void,
                    ctx,
                ) < 0
            {
                child += 1;
            }
            // If parent already >= largest child, the heap property holds.
            if cmp(
                elt(base, width, i) as *const c_void,
                elt(base, width, child) as *const c_void,
                ctx,
            ) >= 0
            {
                return;
            }
            swap_bytes(elt(base, width, i), elt(base, width, child), width);
        }
        i = child;
    }
}

/// In-place heapsort. Builds the heap then repeatedly extracts the max.
///
/// # Safety
/// `base` valid for `n*width` bytes; `cmp(ctx)` callable.
unsafe fn heapsort(base: *mut u8, n: size_t, width: size_t, cmp: CmpFnR, ctx: *mut c_void) {
    if n < 2 || width == 0 {
        return;
    }
    // Build heap: sift down every node with at least one child.
    // Starting index is n/2 - 1 (last internal node); iterate down to 0.
    // SAFETY: indices are heap-bounded.
    unsafe {
        let mut i = n / 2;
        while i > 0 {
            i -= 1;
            sift_down(base, n, width, i, cmp, ctx);
        }
        // Repeatedly swap max-to-end and sift down the reduced heap.
        let mut end = n;
        while end > 1 {
            end -= 1;
            swap_bytes(base, elt(base, width, end), width);
            sift_down(base, end, width, 0, cmp, ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// Public ABI
// ---------------------------------------------------------------------------

/// Adapter that lets `qsort` (2-arg cmp) drive the heapsort that's written
/// against the 3-arg `qsort_r`-style comparator. The original 2-arg cmp
/// pointer is passed through `ctx`; we cast it back inside.
extern "C" fn qsort_2to3_adapter(a: *const c_void, b: *const c_void, ctx: *mut c_void) -> c_int {
    // SAFETY: ctx was set by `qsort()` to the original 2-arg cmp pointer
    // cast to `*mut c_void`. Function pointers and data pointers are the
    // same width on aarch64 LP64; the round-trip is sound.
    let cmp2: CmpFn = unsafe { core::mem::transmute(ctx) };
    cmp2(a, b)
}

/// `void qsort(void *base, size_t nel, size_t width,
///             int (*cmp)(const void *, const void *))`.
///
/// # Safety
/// `base` must be a valid array of `nel` elements of `width` bytes each.
/// `cmp` must be a valid C-ABI function pointer.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn qsort(base: *mut c_void, nel: size_t, width: size_t, cmp: CmpFn) {
    // SAFETY: stuff the 2-arg comparator into the ctx slot of qsort_r.
    unsafe {
        qsort_r(base, nel, width, qsort_2to3_adapter, cmp as *mut c_void);
    }
}

/// `void qsort_r(void *base, size_t nel, size_t width,
///               int (*cmp)(const void *, const void *, void *), void *ctx)`.
///
/// GNU extension; threads `ctx` through to every call of `cmp`.
///
/// # Safety
/// See [`qsort`]. `cmp` may use `ctx` however it wants; we just pass it
/// through.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn qsort_r(
    base: *mut c_void,
    nel: size_t,
    width: size_t,
    cmp: CmpFnR,
    ctx: *mut c_void,
) {
    // SAFETY: heapsort honors the bounds the caller asserts.
    unsafe { heapsort(base as *mut u8, nel, width, cmp, ctx) }
}

/// `void *bsearch(const void *key, const void *base, size_t nel, size_t width,
///                int (*cmp)(const void *, const void *))`.
///
/// Returns a pointer to the matching element or NULL. The array must be
/// sorted in ascending order according to `cmp`.
///
/// # Safety
/// `base` must be a valid array of `nel` elements of `width` bytes each.
/// `cmp` must be a valid C-ABI function pointer.
#[cfg_attr(target_env = "musl", no_mangle)]
pub unsafe extern "C" fn bsearch(
    key: *const c_void,
    base: *const c_void,
    mut nel: size_t,
    width: size_t,
    cmp: CmpFn,
) -> *mut c_void {
    let mut base = base as *const u8;
    // SAFETY: caller asserts the array bounds; we halve until empty.
    unsafe {
        while nel > 0 {
            let try_ = base.add(width * (nel / 2));
            let sign = cmp(key, try_ as *const c_void);
            if sign < 0 {
                nel /= 2;
            } else if sign > 0 {
                base = try_.add(width);
                nel -= nel / 2 + 1;
            } else {
                return try_ as *mut c_void;
            }
        }
    }
    core::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Comparator for ascending i32.
    extern "C" fn cmp_i32_asc(a: *const c_void, b: *const c_void) -> c_int {
        // SAFETY: caller passes pointers to i32 elements.
        unsafe {
            let av = *(a as *const i32);
            let bv = *(b as *const i32);
            (av - bv) as c_int
        }
    }

    // Comparator for descending i32 with a context flag (used to test qsort_r).
    extern "C" fn cmp_i32_dir(a: *const c_void, b: *const c_void, ctx: *mut c_void) -> c_int {
        // SAFETY: ctx is a pointer to a bool indicating "ascending if true".
        unsafe {
            let asc = *(ctx as *const bool);
            let av = *(a as *const i32);
            let bv = *(b as *const i32);
            if asc {
                av - bv
            } else {
                bv - av
            }
        }
    }

    // ---- qsort --------------------------------------------------------

    #[test]
    fn qsort_sorts_ascending() {
        let mut data: [i32; 8] = [5, 2, 8, 1, 9, 3, 7, 4];
        // SAFETY: stack array.
        unsafe {
            qsort(
                data.as_mut_ptr() as *mut c_void,
                data.len(),
                core::mem::size_of::<i32>(),
                cmp_i32_asc,
            );
        }
        assert_eq!(data, [1, 2, 3, 4, 5, 7, 8, 9]);
    }

    #[test]
    fn qsort_handles_already_sorted() {
        let mut data: [i32; 5] = [1, 2, 3, 4, 5];
        // SAFETY: stack array.
        unsafe {
            qsort(data.as_mut_ptr() as *mut c_void, 5, 4, cmp_i32_asc);
        }
        assert_eq!(data, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn qsort_handles_reverse() {
        let mut data: [i32; 6] = [6, 5, 4, 3, 2, 1];
        // SAFETY: stack array.
        unsafe {
            qsort(data.as_mut_ptr() as *mut c_void, 6, 4, cmp_i32_asc);
        }
        assert_eq!(data, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn qsort_handles_duplicates() {
        let mut data: [i32; 7] = [3, 1, 4, 1, 5, 9, 2];
        // SAFETY: stack array.
        unsafe {
            qsort(data.as_mut_ptr() as *mut c_void, 7, 4, cmp_i32_asc);
        }
        assert_eq!(data, [1, 1, 2, 3, 4, 5, 9]);
    }

    #[test]
    fn qsort_zero_and_one_element_no_op() {
        let mut empty: [i32; 0] = [];
        // SAFETY: empty array.
        unsafe {
            qsort(empty.as_mut_ptr() as *mut c_void, 0, 4, cmp_i32_asc);
        }
        let mut single: [i32; 1] = [42];
        // SAFETY: 1-element array.
        unsafe {
            qsort(single.as_mut_ptr() as *mut c_void, 1, 4, cmp_i32_asc);
        }
        assert_eq!(single, [42]);
    }

    // ---- qsort_r ------------------------------------------------------

    #[test]
    fn qsort_r_threads_context_to_comparator() {
        let mut data: [i32; 5] = [5, 2, 8, 1, 9];
        let mut asc: bool = false;
        // Pass &asc as ctx; comparator dereferences it to choose direction.
        // SAFETY: ctx points to a stack-local bool that lives for the whole
        // sort.
        unsafe {
            qsort_r(
                data.as_mut_ptr() as *mut c_void,
                5,
                4,
                cmp_i32_dir,
                core::ptr::addr_of_mut!(asc) as *mut c_void,
            );
        }
        // asc=false → descending order
        assert_eq!(data, [9, 8, 5, 2, 1]);

        asc = true;
        // SAFETY: as above.
        unsafe {
            qsort_r(
                data.as_mut_ptr() as *mut c_void,
                5,
                4,
                cmp_i32_dir,
                core::ptr::addr_of_mut!(asc) as *mut c_void,
            );
        }
        // asc=true → ascending
        assert_eq!(data, [1, 2, 5, 8, 9]);
    }

    // ---- bsearch ------------------------------------------------------

    #[test]
    fn bsearch_finds_existing_element() {
        let data: [i32; 7] = [1, 3, 5, 7, 9, 11, 13];
        for needle in [1, 7, 13, 5] {
            // SAFETY: stack arrays; needle is on the stack.
            let p = unsafe {
                bsearch(
                    core::ptr::addr_of!(needle) as *const c_void,
                    data.as_ptr() as *const c_void,
                    7,
                    4,
                    cmp_i32_asc,
                )
            };
            assert!(!p.is_null(), "looking for {needle}");
            // SAFETY: bsearch returned a pointer into `data`.
            unsafe {
                assert_eq!(*(p as *const i32), needle);
            }
        }
    }

    #[test]
    fn bsearch_returns_null_for_missing_element() {
        let data: [i32; 5] = [2, 4, 6, 8, 10];
        for needle in [1, 5, 9, 11] {
            // SAFETY: stack array; needle on the stack.
            let p = unsafe {
                bsearch(
                    core::ptr::addr_of!(needle) as *const c_void,
                    data.as_ptr() as *const c_void,
                    5,
                    4,
                    cmp_i32_asc,
                )
            };
            assert!(p.is_null(), "{needle} should not be found");
        }
    }

    #[test]
    fn bsearch_empty_returns_null() {
        let data: [i32; 0] = [];
        let needle = 42i32;
        // SAFETY: empty array; bsearch should never deref base.
        let p = unsafe {
            bsearch(
                core::ptr::addr_of!(needle) as *const c_void,
                data.as_ptr() as *const c_void,
                0,
                4,
                cmp_i32_asc,
            )
        };
        assert!(p.is_null());
    }
}
