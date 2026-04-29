# mytilus engineering notes

Running log of design decisions, gotchas, and per-crate deferred work that
isn't obvious from the code or commit history.

## 2026-04-29

Session covered: `mytilus-errno`, `mytilus-prng`, `mytilus-string` (Phase 1
mem*, Phase 2 str*, Phase 3 search/tokenize/case), `mytilus-sys` syscall
layer (`svc #0` asm + `ret` errno classifier), `mytilus-mman` (Phase 1: 12
syscall wrappers).

### Workspace conventions discovered/locked-in

- **Symbol gating**: every C-ABI function that might collide with the host
  libc on `cargo test` uses `#[cfg_attr(not(test), no_mangle)]`. On the
  cross target `not(test)` holds and the unmangled C name is exported; on
  host tests the symbol is mangled and doesn't shadow libc. Without this
  the test binary's runtime calls (e.g. its own internal `mmap`/`memcpy`)
  hit our stubs and abort before tests run.
- **Force-linking rlibs that only provide symbols**: a crate that calls
  into another crate's items only via `extern "C"` (e.g. `mytilus-mman`
  using `mytilus-sys::syscall::ret` which calls `__errno_location`) needs
  `extern crate mytilus_errno;` at the lib root. Cargo declaring the dep
  isn't enough — rustc drops rlibs that aren't named in source.
- **C-ABI fns are `extern "C" fn`, not `unsafe extern "C" fn`**, unless they
  have real preconditions for the caller. Matches relibc / c-ward.
  Internally they may still need `unsafe` blocks. Don't reflexively
  unsafe-tag every C symbol.
- **`c_char` in `mytilus-sys::ctypes` is `u8`** (AArch64 PCS: char is
  unsigned). On macOS hosts `std`'s `c_char` is `i8`, so test helpers that
  call `CStr::as_ptr()` need an explicit `as *const c_char` cast.
- **build-std is per-command, not global**: do NOT set
  `[unstable] build-std=...` in `.cargo/config.toml` — it applies to host
  builds too and causes "duplicate lang item: sized" with the prebuilt host
  sysroot. Pass `-Z build-std=core,alloc,compiler_builtins
  -Z build-std-features=` on the cross-compile command line via Taskfile's
  `BUILD_STD` var.
- **Test mutex for shared global state**: any crate whose tests touch a
  `static mut` (or anything else process-global) must serialize with
  `std::sync::Mutex` in `cfg(test)`. `cargo test` runs in parallel by
  default and a race in test setup looks like a passing run for hours.
  See `mytilus-prng`'s `TEST_LOCK`.
- **Symbol verification beats host tests** for syscall-wrapping crates.
  Their host tests can only check constants; the real signal is
  disassembling the cross rlib and confirming `mov w8, #NR / svc #0`.

### LLVM / codegen gotchas

- **mem*-named functions are NOT recursively rewritten by LLVM**. Loop
  Idiom Recognize has a special "rt-only" exception that skips functions
  literally named `memcpy`/`memset`/`memmove`/`memcmp`. Confirmed via the
  release IR: our byte loops survive optimization rather than being
  rewritten into `call @memcpy`. The output IR carries `norecurse` as
  proof.
- **Object size of byte-loop mem* (release, no LTO)**: memset 24 B,
  memcpy 28 B, memcmp 40 B, memmove 76 B. ~1 KB cheaper than musl's
  hand-tuned `aarch64/memcpy.S` (~1.2 KB) at the cost of being
  order-of-magnitude slower on large copies. PLAN.md commits to swapping
  in the upstream `.S` files for the perf path; the byte loops stay as
  the day-1 correctness path.

### Custom target spec (`aarch64-unknown-linux-mytilus.json`) gotchas

The scaffold's spec had several schema mismatches with the current rustc
serde definitions. Fixed:
- `metadata.tier`: `"custom"` → `3` (it's `u64`, not string).
- `target-pointer-width`: `"64"` → `64` (number, not string).
- Removed `target-c-int-width` (obsolete) and duplicate `pointer-width`.
- `tls-model`: `general-dynamic` → `global-dynamic` (rustc's accepted
  spelling).
- `data-layout`: aligned with LLVM's expected layout for
  `aarch64-unknown-linux-musl` (added `p270/p271/p272` address-space and
  the `Fn32` ABI-stack-alignment terms).

### Per-crate deferred items / TODOs

#### `mytilus-errno`
- `__errno_location` body is a `#[thread_local] static mut ERRNO_VAL`
  stand-in. Upstream reads `__pthread_self()->errno_val`; switch to that
  body once `mytilus-thread` provides `__pthread_self`. The stand-in
  works for the main thread only.

#### `mytilus-prng`
- `random` family uses a `core::sync::atomic::AtomicI32` CAS spinlock as
  a placeholder. Upstream's real lock uses `__lock`/`__unlock` which call
  `futex` via `__wait`. Tagged `TODO(mytilus-thread)`.
- `__random_lockptr` symbol is deferred (`TODO(mytilus-process)`). Only
  consumer is `src/process/fork.c`'s lock-reset path; no point exposing
  before the fork wrapper exists.

#### `mytilus-string`
- `mem*` functions are byte loops; tagged `TODO(perf)` to swap with
  upstream `aarch64/memcpy.S`/`memset.S` when the asm-build plumbing
  lands.
- `strstr` is naive O(n·m); upstream uses Crochemore–Perrin Two-Way for
  ≥5-byte needles plus 2/3/4-byte rolling-hash specializations.
  ~150 LOC of tricky code; deferred until something benchmarks slow.
  `TODO(perf)`.
- `strcasecmp`/`strncasecmp` use ASCII-only fold via inline
  `ascii_tolower`. The `_l` wrappers forward verbatim, which matches
  upstream musl exactly (musl's `__strcasecmp_l` is also a forwarder).
  When `mytilus-locale` provides `tolower`, the non-`_l` path can switch.
  `TODO(locale)`. Note: a minimal ctype subset (~14 fns, ~80 LOC) would
  unblock this — it's a tractable mini-port.

#### `mytilus-sys`
- `__syscall_cp` (cancellation-point variant) intentionally still missing
  here. Must remain handwritten assembly so the cancel handler can
  recognise the exact PC range. Belongs in
  `mytilus-thread/src/asm/syscall_cp.S` per PLAN.md.
- Syscall-number constants: only the mman-related ones are populated in
  `nr.rs`. New ones land lazily as each consumer needs them; we
  deliberately avoid pre-populating the full ~300-entry table.
- `task test:qemu` is required to actually run anything that hits a real
  syscall; pre-existing TODO in the Taskfile.

#### `mytilus-mman`
- `__vm_wait()` is not called on `MAP_FIXED` / `MREMAP_FIXED`.
  Single-threaded behavior is unchanged. `TODO(thread)`.
- `mprotect` does NOT round `addr` down / `addr+len` up to `PAGE_SIZE`.
  Kernel rejects misaligned `addr` with `EINVAL` (matches the spec but
  stricter than musl's leniency). Add musl-style rounding once
  `mytilus-startup` reads `AT_PAGESZ` from the auxv. `TODO(auxv/page-size)`.
- `msync` calls regular `svc`, not `__syscall_cp`. Functionally correct
  for non-cancellable callers; switch when `mytilus-thread`'s asm lands.
  `TODO(thread)`.
- `shm_open` / `shm_unlink` deferred to Phase 2: they're userspace
  wrappers around `open`/`unlink` on `/dev/shm/`, blocked on
  `mytilus-fcntl::open`.

### AArch64 Linux syscall ABI (locked in)

- `x8` = syscall number, `x0..x5` = args, return in `x0`.
- Kernel preserves all registers except `x0`.
- rustc default inline-asm semantics assume memory and flags may be
  modified — no explicit `clobber_abi("system")` or `"memory"` clobber
  needed; just `options(nostack)`.
- Errors come back as `-errno` in `-4096..-1`; `mytilus-sys::syscall::ret`
  classifies and sets errno. The same helper works for pointer-returning
  syscalls because `-1 as *mut c_void` is bit-identical to `MAP_FAILED`.

### Open infrastructure questions

- **No host integration testing for syscalls**: `task test:qemu` is the
  blocker. Until it's wired, syscall-wrapping crates are validated only
  by symbol existence + disassembly (proves the right `svc` is emitted)
  and constant tests. Real behavior coverage is deferred to qemu-aarch64.
- **Page size assumption**: `mytilus-mman` hardcodes 4096. PLAN.md's
  single-target stance says aarch64 only, but real aarch64 kernels can
  run with 16K or 64K pages. We're assuming 4K pages for now; revisit
  when the auxv reader lands and we can read `AT_PAGESZ`.
