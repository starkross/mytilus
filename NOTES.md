# mytilus engineering notes

Running log of design decisions, gotchas, and per-crate deferred work that
isn't obvious from the code or commit history.

## 2026-04-29

Session covered: `mytilus-errno`, `mytilus-prng`, `mytilus-string` (Phase 1
mem*, Phase 2 str*, Phase 3 search/tokenize/case), `mytilus-sys` syscall
layer (`svc #0` asm + `ret` errno classifier), `mytilus-mman` (Phase 1: 12
syscall wrappers), `mytilus-locale` (Phase 1: ctype subset),
`mytilus-time` (Phase 1: clock/sleep syscall wrappers + `timespec`/
`timeval` FFI structs), `mytilus-signal` (Phase 1: sigset_t bit-ops).

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
- **`extern "C" fn` does NOT auto-implement `Fn`/`FnMut`/`FnOnce`** —
  test harnesses that want to take a C-ABI function as a parameter must
  declare it as `f: extern "C" fn(...) -> ...`, not `f: F` where
  `F: Fn(...) -> ...`. The function-item types of `pub extern "C" fn`
  declarations don't unify with the `Fn` traits even though their
  signatures match. (Hit while writing the ctype test harness.)
- **Cycle-avoidance when adding a reverse dep**: scaffold crates often
  list anticipatory deps (e.g. `mytilus-locale` initially listed
  `mytilus-string` for the future `setlocale` port). Before adding a
  *forward* dep that would close the cycle, trim the anticipatory deps
  and document the trim in the Cargo.toml. We did this for the ctype
  port: dropped `mytilus-locale → mytilus-string` so we could add
  `mytilus-string → mytilus-locale`. The future `setlocale` port will
  need to either inline its own minimal byte-loops, or move ctype to
  `mytilus-internal`, before re-adding `mytilus-string` as a dep.
  Same pattern repeated when porting `mytilus-time` and `mytilus-signal` —
  each scaffold listed several anticipatory deps that get trimmed back
  to `mytilus-sys` (+ `mytilus-errno` if the crate sets errno).
- **`<<` after `as <type>` is parsed as generics, not shift**: `1 as
  c_ulong << shift` is a compile error ("interpreted as start of generic
  arguments for `c_ulong`"). Always parenthesize: `(1 as c_ulong) <<
  shift`. Hit while porting `bit_index` in `mytilus-signal`.
- **Inline-asm constraint syntax**: `inlateout("x0") arg => ret`
  declares a register that's both input (with value `arg`) and output
  (writing into `ret`); we use this for syscall return slots where the
  same register holds the syscall arg in and the result out. The
  alternative `inout("x0") val` overwrites `val` in-place, which is
  fine when the caller doesn't need the original value back.
- **FFI struct layout asserted in tests**: every `#[repr(C)]` struct
  that crosses the syscall boundary (`timespec`, `timeval`, `sigset_t`,
  the future `stat`/`pthread_attr_t`/etc.) gets a layout test using
  `core::mem::{size_of, align_of, offset_of}`. Drift here silently
  corrupts every caller; a clean unit-test signal is cheap insurance.

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
- ~~`strcasecmp`/`strncasecmp` use ASCII-only fold via inline
  `ascii_tolower`.~~ **RESOLVED**: case-fold now goes through
  `mytilus_locale::tolower`. The `_l` wrappers still forward verbatim
  (matches upstream — musl's `__strcasecmp_l` is also a forwarder, since
  musl is C-locale-only).

#### `mytilus-sys`
- `__syscall_cp` (cancellation-point variant) intentionally still missing
  here. Must remain handwritten assembly so the cancel handler can
  recognise the exact PC range. Belongs in
  `mytilus-thread/src/asm/syscall_cp.S` per PLAN.md.
- Syscall-number constants: populated lazily in `nr.rs` as consumers
  arrive. After this session: 11 mman NRs + 6 time NRs. We deliberately
  avoid pre-populating the full ~300-entry table.
- `task test:qemu` is required to actually run anything that hits a real
  syscall; pre-existing TODO in the Taskfile.

#### `mytilus-locale`
- Phase 1 ports the **ctype subset only** (`isalpha`/`isdigit`/`isspace`/
  `isupper`/`islower`/`isalnum`/`isxdigit`/`ispunct`/`isprint`/`isgraph`/
  `iscntrl`/`isblank`/`isascii`/`tolower`/`toupper`/`toascii` plus
  `__X_l` and `X_l` locale-aware wrappers). 44 C-ABI symbols total.
- All `__X_l(c, loc)` wrappers ignore `loc` and forward to `X(c)`. This
  matches upstream musl exactly — musl is documented as "C locale only"
  for ctype, and its `__isalpha_l` etc. are themselves forwarders.
- `locale_t` is a placeholder `*mut c_void`. Will be retyped to a real
  struct when locale machinery (`setlocale`, `newlocale`, etc.) lands.
- The `_l` symbol pairs (`__X_l` strong + `X_l` weak alias) are emitted
  via a small `ctype_l!` macro because Rust doesn't have ergonomic
  weak-alias syntax. Both symbols point to the same body; the
  visibility-script linker layer can mark `X_l` weak when we get there.
- Cargo.toml deliberately stripped to just `mytilus-sys` — see the
  cycle-avoidance note in conventions above.
- Bit-twiddling tricks ported verbatim from musl
  (`((unsigned)c|32)-'a' < 26` style) rather than using a 256-entry
  lookup table. Bit-identical results to upstream.

#### `mytilus-time`
- Phase 1 ports the syscall wrappers only: `clock_gettime`/`__clock_gettime`,
  `clock_settime`, `clock_getres`, `clock_nanosleep`/`__clock_nanosleep`,
  `nanosleep`, `gettimeofday`, `time` (9 C-ABI symbols).
- `timespec` and `timeval` are defined here as `#[repr(C)]` structs and
  re-used by everyone else (mytilus-thread for `pthread_*_timedwait`,
  mytilus-net for `SO_RCVTIMEO`, future stdio, etc.).
- **`clock_nanosleep` returns errno DIRECTLY** (positive on failure, 0 on
  success), NOT the standard `-1 + errno-set` convention. Its impl uses
  `-r as c_int` rather than `ret(r)`. `nanosleep` then negates and runs
  through `ret` to translate to the standard convention. Mis-porting this
  would silently break every caller.
- The cross-target codegen for `nanosleep` shows the win: LLVM inlined
  `__clock_nanosleep` into it, so `nanosleep` is one `svc` plus errno
  classification — no indirect call.
- `TODO(perf, vDSO)`: upstream `clock_gettime` tries the kernel-provided
  vDSO (`__kernel_clock_gettime`) first and falls back to `svc`. We
  always go through `svc`. Wiring vDSO needs an auxv reader from
  `mytilus-startup` to find the vDSO base.
- `TODO(thread/cancel)`: `clock_nanosleep`/`nanosleep` use plain `svc`,
  not `__syscall_cp`. Switch when `mytilus-thread`'s asm lands.
- Deferred to later phases: `mktime`, `gmtime`, `localtime`, `strftime`,
  `strptime`, `ctime`, `asctime`, `difftime`, `timer_*`, TZif parser,
  `__tz` machinery — all need malloc, fcntl, or substantial table data.

#### `mytilus-signal`
- Phase 1 ports just the **sigset_t bit-manipulators**: `sigemptyset`,
  `sigfillset`, `sigaddset`, `sigdelset`, `sigismember`, `sigorset`,
  `sigandset`, `sigisemptyset` (8 C-ABI symbols). Pure bit-twiddling on
  a fixed 128-byte struct, no syscalls.
- `sigset_t` shape (kernel ABI): `struct { unsigned long __bits[16]; }`
  on LP64 = 128 bytes. Only `__bits[0]` (one `u64`) carries real signal
  bits — the remaining 120 bytes are reserved padding for future
  signals. `SST_SIZE` (loop count for sigorset/sigandset/sigisemptyset)
  is 1 on our target.
- **Three reserved signals**: `sigaddset`/`sigdelset` reject signals
  32, 33, 34 with `EINVAL`. musl uses these internally:
  - 32 = `SIGCANCEL` (pthread_cancel)
  - 33 = `SIGSYNCCALL` (synchronous broadcast across threads)
  - 34 = reserved for `setxid` / future use
  `sigfillset`'s magic constant `0xfffffffc7fffffff` masks out these
  three bits. `sigismember` does NOT reject reserved signals — apps can
  still query whether they're set.
- `sigemptyset` upstream only zeroes `__bits[0]` on LP64+_NSIG=65 (the
  `_NSIG > 65` path is dead). We mirror that: callers must always
  zero-init themselves before passing if they care about the upper
  120 bytes.
- This is the canonical `sigset_t` that `sigaction`, `sigprocmask`,
  `pthread_sigmask`, `sigtimedwait`, etc. will all consume — locking
  the layout in early matters.
- Deferred to later phases: signal-handler installation (`sigaction`,
  `signal`, `bsd_signal`), masking (`sigprocmask`, `pthread_sigmask`),
  delivery (`raise`, `kill`, `tkill`, `tgkill`), the `restore`
  trampoline, `siginfo_t`, real-time signal support, `sigsuspend`,
  `sigwait*`. Those all need real syscalls plus the cancellation/thread
  machinery.

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
