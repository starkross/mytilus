# mytilus engineering notes

Running log of design decisions, gotchas, and per-crate deferred work that
isn't obvious from the code or commit history.

## 2026-04-29

Session covered: `mytilus-errno`, `mytilus-prng`, `mytilus-string` (Phase 1
mem*, Phase 2 str*, Phase 3 search/tokenize/case), `mytilus-sys` syscall
layer (`svc #0` asm + `ret` errno classifier), `mytilus-mman` (Phase 1: 12
syscall wrappers), `mytilus-locale` (Phase 1: ctype subset),
`mytilus-time` (Phase 1: clock/sleep syscall wrappers + `timespec`/
`timeval` FFI structs), `mytilus-signal` (Phase 1: sigset_t bit-ops),
`mytilus-stdlib` (Phase 1: abs/div/qsort/bsearch — first callback FFI),
`mytilus-fcntl` (Phase 1: open/openat/creat/fcntl/posix_fadvise/posix_fallocate
— first variadic FFI), CI workflow.

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
- **Callback FFI shape**: `extern "C" fn(*const c_void, *const c_void)
  -> c_int` for 2-arg comparators (qsort/bsearch),
  `extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int`
  for the 3-arg `_r` form. Same shape `pthread_create`'s start-routine
  (`extern "C" fn(*mut c_void) -> *mut c_void`) and `atexit`/`tss_create`
  destructors will use. `qsort` adapts its 2-arg cmp to the 3-arg
  heapsort by passing the function pointer itself through `ctx` and
  using `core::mem::transmute` to round-trip data ↔ function pointers
  (sound on aarch64 LP64 because both are 64-bit).
- **Struct return by value across the C ABI**: `div_t`/`ldiv_t`/
  `lldiv_t`/`imaxdiv_t` are returned by value from `div`/`ldiv`/etc.
  `extern "C" fn(...) -> div_t` Just Works in Rust — the AArch64 PCS
  passes small POD structs (≤16 bytes) in `x0`/`x1`. Verified by the
  layout tests; no special handling needed.
- **`a.wrapping_neg()` for `abs(INT_MIN)`**: Rust's `-a` panics in debug
  mode on `i32::MIN` (overflow). C's `-a` is implementation-defined and
  works out to `INT_MIN` on 2's-complement. Use `wrapping_neg()` to
  match the observable upstream behavior without panicking.
- **Variadic FFI (`#![feature(c_variadic)]`)**: declare the function with
  trailing `mut args: ...`, then `args.arg::<T>()` extracts one variadic
  slot at a time. Default argument promotion still applies on the C side
  (types narrower than `int` promote to `int`; `float` promotes to
  `double`); on Rust's side we just pass the post-promotion type to
  `arg::<T>()`. `mode_t = u32 = c_uint` is already int-width so no
  promotion needed. `unsigned long` (`fcntl`'s arg slot) is `c_ulong`.
  Verified by inspection: variadic functions emit a ~192-byte prologue
  on AArch64 that spills x0..x7 (64 B) and q0..q3 (64 B) into a "variadic
  register save area" per AAPCS64. No libc helpers (`__va_arg` etc.) get
  pulled in — pure inline lowering. The 192 B is acceptable overhead for
  syscall wrappers; worth knowing for hot variadics like `printf`.

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
  arrive. After this session: 11 mman NRs + 6 time NRs + 5 fcntl NRs
  (= 22). We deliberately avoid pre-populating the full ~300-entry table.
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

#### `mytilus-stdlib`
- Phase 1 ports `abs`/`labs`/`llabs`/`imaxabs`, `div`/`ldiv`/`lldiv`/
  `imaxdiv`, `qsort`/`qsort_r`, `bsearch` (11 C-ABI symbols) plus the
  `div_t`/`ldiv_t`/`lldiv_t`/`imaxdiv_t` return-by-value structs.
- **`qsort` is heapsort, NOT smoothsort**. Upstream uses smoothsort
  (~230 LOC of Leonardo-heap state-machine code) for its near-O(n)
  behavior on already-sorted input; we use plain heapsort (~50 LOC,
  same O(n log n) worst case, in-place, allocation-free). Tagged
  `TODO(perf)` to swap in upstream's smoothsort once a bench harness
  shows it matters. Public ABI is unchanged either way.
- `qsort_r` is the canonical 3-arg-cmp form; `qsort` adapts to it via
  a static `qsort_2to3_adapter` that recovers the original 2-arg cmp
  pointer from `ctx` via `core::mem::transmute`. Standard libc trick;
  sound on aarch64 LP64 because data and function pointers are both
  64-bit.
- `swap_bytes` is a byte-by-byte loop. Tagged `TODO(perf)` to upgrade
  to word-stride / chunked-buffer swap (10–100× faster for wide
  elements). Currently fine for correctness.
- `intmax_t` aliased locally to `i64` since `mytilus-sys::ctypes`
  doesn't have it. Promote to a shared alias when another consumer
  needs it.
- Deferred to later phases (need malloc / env / stdio / string parsing):
  `strtol` family, `atoi`/`atol`/`atoll`, `strtod`/`atof`, env
  (`getenv`/`setenv`/`putenv`/`unsetenv`), exit/atexit, `mblen` family,
  `realpath`, `mkstemp`/`mkdtemp`, `system`.

#### `mytilus-fcntl`
- Phase 1 ports `open`, `openat`, `creat`, `fcntl`, `posix_fadvise`,
  `posix_fallocate` (6 C-ABI symbols). Three of them (`open`, `openat`,
  `fcntl`) are **C-variadic**.
- **AArch64 specialization**: kernel has no `SYS_open`. `open(path, …)`
  routes through `openat(AT_FDCWD, …)`. Both `open` and `openat` share
  an internal `openat_inner` helper.
- **`O_LARGEFILE` is OR-ed in unconditionally**: the kernel ignores it
  on 64-bit, but C callers expect a 64-bit-aware handle so we mirror
  upstream and OR it in for both `openat_inner` and `fcntl(F_SETFL)`.
- **`O_CLOEXEC` belt-and-suspenders**: even though the kernel honors
  `O_CLOEXEC` on `openat` since 2.6.23, we also re-apply
  `fcntl(fd, F_SETFD, FD_CLOEXEC)` after a successful open with
  `O_CLOEXEC`. Mirrors upstream — defends against weird old kernels and
  is essentially free on modern ones.
- **`posix_fadvise`/`posix_fallocate` return positive errno**: same
  POSIX gotcha as `clock_nanosleep`. Their impl uses `-r as c_int`
  rather than `ret(r)`. Mis-porting silently breaks every caller.
- Two upstream workarounds **dropped** in `fcntl`:
  - `F_GETOWN → F_GETOWN_EX` translation (upstream uses to disambiguate
    process-group returns from errors). We pass `F_GETOWN` directly.
    Tagged `TODO(compat)`.
  - `F_DUPFD_CLOEXEC` → `F_DUPFD + F_SETFD` fallback (for kernels lacking
    the cloexec variant). We pass through. Tagged `TODO(compat)`.
- **`TODO(thread/cancel)`**: `open`/`openat`/`fcntl(F_SETLKW)` are
  cancellation points upstream (use `__syscall_cp`). We use plain `svc`.
  Switch when `mytilus-thread`'s asm lands.
- 7 NRs added to `mytilus-sys::nr`: `SYS_fcntl`, `SYS_fallocate`,
  `SYS_openat`, `SYS_close`, `SYS_fadvise64` (5 new this round; 2
  previously).

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

### CI / GitHub Actions

`.github/workflows/ci.yml` runs three parallel jobs on `ubuntu-latest`,
all driven through the Taskfile so the local `task ci` command and CI
exercise the same gates:

- **fmt** — `task fmt:check`. No cargo deps required; super fast.
- **host** — `task clippy:host` + `task test`. Caches the host target dir.
  Tests can't exercise the syscall path (stubs panic outside aarch64-linux);
  they cover all the pure / pointer / struct-layout code, which is the
  bulk of what we have.
- **cross** — `task check` + `task clippy` + `task build` against
  `aarch64-unknown-linux-mytilus.json` with `-Z build-std`. Caches the
  cross target dir separately. Catches ABI / target-spec / asm
  regressions before they hit a real build.

Rust toolchain is auto-installed from `rust-toolchain.toml` via
`actions-rust-lang/setup-rust-toolchain@v1` (respects the `nightly-…` pin
plus all components). `arduino/setup-task@v2` installs Task itself.
`Swatinem/rust-cache@v2` provides the cargo registry / target cache.

**Pre-existing Taskfile YAML bug fixed in this session**: `cmds:` items
that were `- echo "TODO: …"` (with a literal `:` inside the unquoted
YAML scalar) made Task's parser interpret the line as a map entry and
fail with `invalid keys in command`. Fix: either single-quote the whole
command (`- 'echo "..."'`) or replace the `:` with a different
separator. Six such lines fixed across `test:qemu` / `symbols:list` /
`symbols:diff` / `abi:diff` / `install` / `headers:install`. None had
ever been run before, so the bug was latent.

### Open infrastructure questions

- **No host integration testing for syscalls**: `task test:qemu` is the
  blocker. Until it's wired, syscall-wrapping crates are validated only
  by symbol existence + disassembly (proves the right `svc` is emitted)
  and constant tests. Real behavior coverage is deferred to qemu-aarch64.
- **Page size assumption**: `mytilus-mman` hardcodes 4096. PLAN.md's
  single-target stance says aarch64 only, but real aarch64 kernels can
  run with 16K or 64K pages. We're assuming 4K pages for now; revisit
  when the auxv reader lands and we can read `AT_PAGESZ`.
