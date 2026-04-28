# mytilus — Reimplementation Plan

A plan to reimplement [musl libc](http://www.musl-libc.org/) (v1.2.6) in Rust, scoped to:

- **Target arch:** `aarch64` only (ARMv8-A 64-bit). No 32-bit ARM, no x86, no RISC-V, no MIPS, no PowerPC, no s390x.
- **Bitness:** **64-bit only**. All ILP32 / time32 / off32 compatibility shims are dropped.
- **Kernel ABI:** Linux ≥ 4.1 (the floor that pure-aarch64 syscall-only operation is sane on; pin to ≥ 5.10 LTS for `clone3`, `faccessat2`, statx-everywhere).
- **Drop:** the entire `arch/{arm,i386,m68k,microblaze,mips,mips64,mipsn32,or1k,powerpc,powerpc64,riscv32,riscv64,s390x,sh,x32,x86_64,loongarch64,generic}` tree, `compat/time32`, all 32-bit-off_t syscalls, `fcntl64`/`stat64`/`mmap2`/`_llseek` paths, FDPIC, `microblaze`/`sh` quirks.

---

## 1. Goals & Non-Goals

### Goals
1. **ABI-compatible** drop-in replacement for upstream musl on `aarch64-unknown-linux-musl`. A binary linked against musl 1.2.6 must continue to run unchanged when relinked against mytilus.
2. **Memory-safe in the parts that can be**: no UB in pure data-structure code (parsing, formatting, collections, allocator metadata). Use `unsafe` only at the syscall / FFI / TLS boundary.
3. **No `std`, no `core::alloc::GlobalAlloc` dependency on libc** — mytilus *is* the libc. We use only `core` plus a minimal handwritten runtime.
4. **Single artifact:** produce `libc.so` (dynamic) and `libc.a` (static), `ld-musl-aarch64.so.1`, `crt1.o / crti.o / crtn.o / Scrt1.o / rcrt1.o`. Same file names, same `DT_SONAME`.
5. **Static and dynamic linking** both first-class. PIE and non-PIE.
6. **Conformance:** ISO C11 + POSIX 2008 base + the Linux/glibc extensions musl already exposes (BSD-isms, `getrandom`, `pthread_setname_np`, `memfd_create`, etc.).
7. **Maintain the musl design ethos:** small code, small data, fail-safe, no surprises.

### Non-Goals
- 32-bit support of any kind (no `time32`, no `off_t == long`, no ILP32).
- Architectures other than `aarch64`.
- Big-endian aarch64 (`aarch64_be`) — we only support little-endian. This excludes a few exotic embedded SoCs and is fine.
- ABI tag compatibility with **glibc** (we keep musl's symbol set, not glibc's versioned symbols).
- Sandbox/seccomp policy. That is an application concern.
- Replacement of the build system (we'll target Cargo + a thin Make wrapper that mirrors `./configure && make`).

---

## 2. Why Rust here is non-trivial

A libc has structural constraints that the Rust ecosystem normally papers over by *depending on a libc*. We are removing that floor. Concretely:

| Constraint | Consequence |
|---|---|
| **`#![no_std]` everywhere.** | No `String`, `Vec`, `HashMap`, no `std::sync`, no panic-unwind by default. We bring our own collections in `internal/`. |
| **No global allocator at startup.** | The dynamic linker (`ld-musl`) runs **before** `malloc` is initialized. Bootstrap code must use `mmap` directly. |
| **`#[panic_handler]` is ours.** | Panics in libc must abort cleanly via `SYS_exit_group`, never unwind into user code. |
| **Symbol names are fixed by C ABI.** | Every public function is `#[no_mangle] pub extern "C" fn`. No name mangling, no Rust-level generics on the boundary. |
| **`errno` is per-thread.** | TLS access has to work *before* `pthread_create` ever runs and even from within the dynamic linker. We can't lean on `#[thread_local]` until the TLS image exists. |
| **Static initialization order.** | Rust's `static`s with non-const initializers don't exist. Anything that needs init runs from `__libc_start_main`. |
| **Cancellation points.** | A thread cancelled mid-syscall has to unwind specific assembly stack frames. This is the one place we must keep handwritten asm and structured *exactly* like musl's `syscall_cp.s`. |
| **`fork()` async-signal-safety.** | Post-fork in the child, only async-signal-safe code may run — including any allocator metadata fixup. Rust's drop glue is hostile here. |
| **No `std::process::abort`.** | We *are* `abort(3)`. |

The plan accounts for each of these. Most of the libc surface is straightforward; the dangerous third (dynamic linker, thread/TLS bootstrap, signal restore, cancellation, `fork`) is where we keep handwritten asm and the tightest review.

---

## 3. Architecture: workspace layout

```
mytilus/
├── Cargo.toml                  # virtual workspace
├── rust-toolchain.toml         # pinned nightly (see §4)
├── Makefile                    # `make install` wrapper around cargo
├── build.rs                    # generates linker scripts, version map
│
├── crates/
│   ├── mytilus-sys/               # Raw aarch64 Linux syscall layer + register defs
│   ├── mytilus-asm/            # The (small) handwritten asm bits, as a build artifact
│   ├── mytilus-internal/          # Shared no_std types: lock, atomics, lists, futex
│   ├── mytilus-startup/           # crt1, crti, crtn, Scrt1, rcrt1, __libc_start_main
│   ├── mytilus-errno/             # __errno_location, syscall return classification
│   ├── mytilus-string/            # mem*, str*, wmem*, wcs*  (perf-critical hot path)
│   ├── mytilus-stdio/             # FILE, buffered I/O, printf/scanf families
│   ├── mytilus-stdlib/            # qsort, bsearch, strtol family, env, exit, atexit
│   ├── mytilus-malloc/            # mallocng port (the *only* allocator)
│   ├── mytilus-math/              # libm — split into mytilus-math-soft and mytilus-math-neon
│   ├── mytilus-time/              # clock, time, mktime, strftime, tz
│   ├── mytilus-locale/            # locale, iconv, multibyte, message catalog
│   ├── mytilus-thread/            # pthreads, sem, TLS, cancellation
│   ├── mytilus-signal/            # sigaction, signal, sigset, restore trampoline
│   ├── mytilus-fcntl/             # open/openat/fcntl
│   ├── mytilus-unistd/            # read/write/lseek/dup/...
│   ├── mytilus-mman/              # mmap/munmap/mprotect/madvise/mlock
│   ├── mytilus-process/           # fork, vfork, posix_spawn, wait*
│   ├── mytilus-fs/                # dirent, stat (statx-backed), realpath, glob, fnmatch
│   ├── mytilus-net/               # sockets, getaddrinfo, getnameinfo, resolver, DNS
│   ├── mytilus-passwd/            # getpwnam, NSS-like file backends
│   ├── mytilus-crypt/             # crypt(), crypt_r(), bcrypt, sha-crypt, des fallback
│   ├── mytilus-prng/              # rand/random/erand48/getrandom
│   ├── mytilus-search/            # tsearch, hsearch, lsearch
│   ├── mytilus-regex/             # POSIX regex (TRE-like)
│   ├── mytilus-ipc/               # SysV ipc, POSIX mq, POSIX shm, POSIX sem
│   ├── mytilus-aio/               # aio_*
│   ├── mytilus-ldso/              # the dynamic linker (`ld-musl-aarch64.so.1`)
│   └── mytilus/              # the umbrella crate that re-exports everything
│
├── headers/                    # C headers we ship (mostly verbatim from musl)
│   ├── bits/                   # aarch64-only bits/* (one set, not 19)
│   └── ...
│
├── linker/                     # version scripts, dynamic.list
│
└── tools/                      # mksyscalls, mkalltypes, mkerrnoh — Rust replacements
```

Why a workspace and not one giant crate?

1. **Compile time** — touching `printf` should not recompile the resolver.
2. **Boundary clarity** — `mytilus-ldso` *cannot* depend on `mytilus-malloc`; the workspace makes that a hard error rather than a code-review note.
3. **Test isolation** — most crates can be tested on the host with a stubbed `mytilus-sys`. A monolith would force every test to be a full integration test.

---

## 4. Toolchain & Cargo configuration

### Toolchain
- **Rust:** pinned nightly (we need `#![feature(naked_functions)]`, `#![feature(asm_const)]`, `#![feature(c_unwind)]`, `#![feature(thread_local)]` on a custom target, `#![feature(linkage)]` for weak symbols, `#![feature(used_with_arg)]`, `#![feature(linker_messages)]`). Pin via `rust-toolchain.toml`; re-evaluate on every Rust release.
- **Custom target:** `aarch64-unknown-linux-mytilus.json` — based on `aarch64-unknown-linux-musl` but `dynamic-linking = true`, `crt-static = false` by default, `panic-strategy = "abort"`, `relocation-model = "pic"`, `tls-model = "local-exec"` for the static libc / `"general-dynamic"` for the dynamic one.
- **`build-std`:** we build `core`, `alloc`, and `compiler_builtins` from source via `-Z build-std`. We **do not** ship `alloc` to users — it's an internal dependency for the malloc crate's data structures, and we tree-shake. `compiler_builtins` is critical for `__aarch64_*` helpers (`__aeabi_*` is irrelevant on AArch64).
- **`-Z build-std-features=compiler-builtins-mem`:** off — we provide our own `memcpy`/`memset`/`memmove`/`memcmp` and they need to be the canonical symbols.
- **No unwinding.** `panic = "abort"`, no `eh_personality`, no `.eh_frame` from Rust code (we keep `.eh_frame` only for assembly-bridged C++ exception backtraces in user code).

### Cargo workspace
- `resolver = "2"`.
- `[profile.release]`: `opt-level = "s"` (musl prizes size), `lto = "fat"`, `codegen-units = 1`, `debug = "line-tables-only"`, `overflow-checks = false` (the C ABI surface promises wraparound semantics and `-fwrapv`-style behavior; explicit `wrapping_*` everywhere we mean it).
- `[profile.release-perf]`: opt-in `opt-level = 3` profile for the math/string crates only.
- `panic = "abort"` workspace-wide.

### Forbidden in the workspace
- `extern crate std`
- `alloc::*` outside `mytilus-malloc`'s own implementation crate
- `#[derive(Debug)]` in the public surface (it pulls in formatting which pulls in allocation)
- Any dependency on crates.io. Period. The dependency graph is `core`, `compiler_builtins`, and us. Any external crate is a supply-chain liability for a libc.

---

## 5. Boundary & ABI rules

These are non-negotiable invariants enforced by code review and lint:

1. Every public symbol is declared:
   ```rust
   #[no_mangle]
   pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, ...) -> c_int { ... }
   ```
   `extern "C-unwind"` only where the C standard explicitly allows unwinding (nowhere, in our scope).
2. **`c_int`, `c_long`, `c_char`, `size_t`, `off_t`, `time_t`** come from a single `mytilus-sys::ctypes` module. `off_t` is `i64`. `time_t` is `i64`. `size_t` is `usize`. No conditional definitions — we are 64-bit-only.
3. Public functions never panic. A bug that would panic must `__libc_fatal()` (write a message to fd 2 and `SYS_exit_group`).
4. Public functions never allocate via Rust's `Box::new` / `Vec::new`. They call our `malloc` / `mmap` directly. (`alloc` is allowed only inside `mytilus-malloc`'s internal data structures, which sit on a static arena.)
5. Public functions take `*const c_char`, not `&CStr`. Conversion happens at the boundary and is bounded by `strnlen` against a sane cap (PATH_MAX, NAME_MAX).
6. A weak symbol is `#[linkage = "weak"] #[no_mangle]`. We have a CI lint that checks every musl weak symbol stays weak (e.g., `__init_libc`, `__lockfile`, `__stdio_exit`).
7. Version script (`linker/musl.ver`) lists every exported symbol. CI diffs the `.so` symbol set against `nm -D` of upstream musl built on aarch64 and fails on any divergence.

---

## 6. Subsystem-by-subsystem plan

### 6.1 `mytilus-sys` — the syscall floor
- AArch64-only. `svc #0`, register convention `x8 = nr, x0..x5 = args, return in x0`.
- Implemented via inline asm with `naked_functions` for the cancellation-point variant; everything else uses `core::arch::asm!` from a regular function.
- **`__syscall_cp`** must remain handwritten asm (`syscall_cp.s` analogue) so the cancellation handler can recognize the pre-syscall instruction by exact PC. We keep this as a `.S` file built via `cc` crate and link it in. Rust naked functions are *almost* good enough but the cancel handler relies on exact instruction offsets (`__cp_begin`, `__cp_end`, `__cp_cancel`).
- Syscall numbers come from `arch/aarch64/bits/syscall.h.in`. We codegen `pub const SYS_*: c_long = N;` with a build script (`tools/mksyscalls`).
- **Drop** `__SYSCALL_LL_E`/`__SYSCALL_LL_O` (those are 32-bit-pair packers). Drop `IPC_64`. Drop the `socketcall` indirection (aarch64 has direct socket syscalls).
- **VDSO:** keep the `__kernel_clock_gettime` / `__kernel_gettimeofday` fast paths. Lookup at libc init via `AT_SYSINFO_EHDR`. Rust function pointers stored in `static AtomicPtr`s.
- **Errno classification:** the kernel returns `-4096..-1` as errors. We centralize that here, not in 200 callsites.

### 6.2 `mytilus-startup` — crt and `__libc_start_main`
- `crt1.o`, `crti.o`, `crtn.o`, `Scrt1.o` (PIE), `rcrt1.o` (statically-linked-PIE) are produced as object files by compiling tiny `#![no_std]` `#![no_main]` Rust crates with custom linker fragments. Some of these *must* remain assembly because they define the entry point and there is no stack frame yet (`crti.s` provides `_init`/`_fini` prologue inside `.init`/`.fini` sections — Rust cannot emit that section layout cleanly). We keep:
  - `crt/aarch64/crti.s`, `crt/aarch64/crtn.s` — verbatim from upstream.
  - `crt1.c`, `Scrt1.c`, `rcrt1.c` → Rust, calling into `__libc_start_main`.
- `__libc_start_main`:
  1. Save `argc`, `argv`, `envp`, `auxv` in `__libc`.
  2. Process `AT_*` aux entries (`AT_PAGESZ`, `AT_RANDOM`, `AT_HWCAP`, `AT_HWCAP2`, `AT_SYSINFO_EHDR`, `AT_SECURE`).
  3. Initialize TLS (calls into `mytilus-thread`).
  4. Initialize stdio, locale.
  5. Run `.init_array`.
  6. Call `main`.
  7. Call `exit` with the result.
- The auxv parser is a perfect candidate for safe Rust: it's a slice of `(u64,u64)` pairs; we expose `auxv(AT_PAGESZ) -> Option<u64>`.

### 6.3 `mytilus-errno`
- `__errno_location` returns `&mut self.errno` from the current thread's `pthread` struct.
- Pre-TLS-init path: returns address of a static `static mut INITIAL_ERRNO: c_int`.
- Switchover happens atomically inside `__init_tp`.

### 6.4 `mytilus-string` — the perf hot path
- `memcpy` and `memset` **stay as `.S`** for AArch64. Upstream's `src/string/aarch64/memcpy.S` and `memset.S` are tuned for Cortex-A57 / A72 / Neoverse and beat anything LLVM emits. Pull them in via `cc` crate, link with the same names.
- Everything else (`strlen`, `strchr`, `strcmp`, `memchr`, etc.) → Rust. Use `core::simd` (portable SIMD, stable on AArch64 NEON) or `std::arch::aarch64` intrinsics for hot paths. Pure-scalar fallback for the cold ones.
- Public boundary: `pub unsafe extern "C" fn strlen(s: *const c_char) -> size_t`. Internally calls `strlen_neon(s)` after a misalignment prefix.
- `bcmp`, `bcopy`, `bzero`, `index`, `rindex` are obsolete BSD aliases — keep them as one-liners delegating to the modern names.

### 6.5 `mytilus-stdio`
- The `FILE` struct **layout is part of the ABI** (people stack-allocate it via `__fmemopen` and friends, and old binaries embed offsets). We `#[repr(C)]` a struct with the exact field order from `src/internal/stdio_impl.h`.
- Locking: each `FILE` carries a lock (futex). Recursive locking via `flockfile`/`funlockfile` matches musl's `__lockfile` mechanism.
- `printf` / `scanf`: re-port from `src/stdio/vfprintf.c` and `vfscanf.c`. These are state-machine parsers — clean Rust port, all in safe code modulo the va_list reads. AArch64 `va_list` is the `__va_list` AAPCS64 struct (`stack`, `gr_top`, `vr_top`, `gr_offs`, `vr_offs`); we model it with `#[repr(C)]` and `core::ffi::VaList` once stable, otherwise hand-rolled.
- `floatscan` / `intscan` — port faithfully; these are hot and tested heavily.
- `__stdio_exit`, line-buffered TTY detection (via `isatty` → `TCGETS`), `__towrite`/`__toread` state machine — all preserved.

### 6.6 `mytilus-malloc` — `mallocng`
- We **port `mallocng`** (`src/malloc/mallocng/`) and not `oldmalloc`. mallocng is the modern allocator (slot-based, hardened metadata), and there's no reason to carry the old one.
- Rust port keeps the same data structures (`meta`, `group`, `ctx`) but uses `repr(C)` and explicit alignment.
- **Bootstrap:** the dynamic linker (`ld-musl`) runs before `malloc`, so `dlmalloc-style` early calls go through `lite_malloc`. `lite_malloc` is a one-way bump allocator over `mmap`; we keep it as a separate module. After `__libc_start_main`, the symbol `malloc` is rebound (via the `replaced.c`-style mechanism) from `lite_malloc` to `mallocng`.
- `posix_memalign`, `aligned_alloc`, `memalign`, `pvalloc`, `valloc` — wrappers on `mallocng`.
- `reallocarray`, `mallinfo` (stub), `malloc_usable_size` — wrappers.
- Free-list pointer-mangling stays. UAF mitigations stay.
- We keep mallocng's `malloc_replaced` weak hook so `LD_PRELOAD`-style allocators (jemalloc, mimalloc) still override us cleanly.

### 6.7 `mytilus-math` — libm
- Most functions are pure Rust ports of musl's polynomial / table-driven implementations. There's a community crate `libm` that already does this (BSD-licensed) — **we do not depend on it** (no crates.io) but we may **vendor specific files** with attribution if their licensing permits and the implementation matches musl's expected accuracy.
- Functions where AArch64 has a single instruction (`fsqrt`, `fabs`, `frintn`, `fmaxnm`, `fminnm`, `fma`) get an intrinsic-based fast path:
  - `sqrt` / `sqrtf` → `vsqrtq_f64` / `vsqrtq_f32` style or just `core::arch::aarch64::vsqrt_f64`.
  - `floor` / `ceil` / `round` / `trunc` / `nearbyint` / `rint` → `frintm`, `frintp`, `frinta`, `frintz`, `frinti`, `frintn` respectively.
  - `fma` / `fmaf` → `fmadd`.
  - `fmax`/`fmin`/`fmaxnm`/`fminnm` → `fmaxnm` etc.
- Drop the soft-float fallbacks; AArch64 base mandates VFP/NEON. (`-mgeneral-regs-only` callers are not supported. We document this.)
- `fenv` (`fegetround`, `fesetround`, exceptions) → port of `src/fenv/aarch64/fenv.s` to Rust using `mrs`/`msr` on `FPCR`/`FPSR`. This is fine in inline asm.
- Long double on aarch64 is IEEE binary128 (quad). musl's `ld128` files apply. We port them faithfully.
- Complex math: thin port of `src/complex/`.

### 6.8 `mytilus-time`
- All clock-* go through VDSO when available. Backup: `clock_gettime` syscall.
- `time_t` is `i64` everywhere. We do not implement `time32` shims. Year-2038 is not a problem we have.
- `mktime`, `localtime_r`, `strftime`, `strptime`: faithful ports. The TZ database parser (`__tzset`) reads `/etc/localtime` (TZif format) — straightforward Rust.
- `nanosleep`, `clock_nanosleep` use `SYS_clock_nanosleep` directly.
- Interval timers, `timer_create`, `timer_settime` — wrap `SYS_timer_create` with the SIGEV thread mode requiring a helper thread (port of `src/time/timer_create.c`).

### 6.9 `mytilus-thread` — pthreads & TLS
This is the trickiest crate. Rough outline:
- **`struct pthread`** layout is fixed (musl's `src/internal/pthread_impl.h`). It must keep `self`, `dtv`, `tid`, `errno`, `cancel`, lock list at the same offsets — applications and gdb know these offsets.
- **TLS:** AArch64 uses TLS Variant I. The TCB lives at `tpidr_el0`. Initial TLS image comes from `PT_TLS` segment(s). For dynamic loading, we implement TLSDESC (`__tlsdesc_static`, `__tlsdesc_dynamic` — in **assembly**, because the calling convention only clobbers `x0` and the condition codes; Rust cannot express that without a naked function and even there it's fragile). Keep `src/ldso/aarch64/tlsdesc.s` essentially as-is.
- **`__set_thread_area`** on aarch64 just sets `tpidr_el0` via `msr tpidr_el0, x0`. Trivial.
- **`clone` wrapper** (`src/thread/aarch64/clone.s`) stays as `.S`. Reason: between `clone` returning in the child and the child calling its start function, there is no valid C/Rust call frame.
- **`__unmapself`** stays as `.S` for the same reason: it must `munmap` its own stack and then `exit`, with no stack to return to between the two syscalls.
- **Synchronization:** `pthread_mutex_t`, `pthread_cond_t`, `pthread_rwlock_t`, `sem_t` are all built on `futex` (`SYS_futex`, `FUTEX_WAIT`/`FUTEX_WAKE`/`FUTEX_REQUEUE`/`FUTEX_CMP_REQUEUE`). Port faithfully — the algorithms here are subtle (priority inheritance, robust mutexes, requeue-PI). Use `core::sync::atomic` with explicit orderings; document each `Ordering::Relaxed` / `Acquire` / `Release` choice.
- **Cancellation:** the model is unchanged from musl — `pthread_setcancelstate` flips a flag; cancellation points check it after their syscall. The deferred-cancel path requires the assembly trampoline mentioned in 6.1.
- **`pthread_create`** allocates a stack with `mmap(MAP_STACK)`, sets up the TCB, calls `clone(CLONE_VM|CLONE_FS|CLONE_FILES|CLONE_SIGHAND|CLONE_THREAD|CLONE_SYSVSEM|CLONE_SETTLS|CLONE_PARENT_SETTID|CLONE_CHILD_CLEARTID)`. We can move to `clone3` (`SYS_clone3`) on kernels ≥ 5.5; fall back to `clone` otherwise.
- **TSD (`pthread_key_create`)** — port directly.

### 6.10 `mytilus-signal`
- `sigaction` plus the **signal restorer trampoline** (`src/signal/aarch64/restore.s`) stays as assembly. The kernel jumps to it on signal return; it must be exactly two instructions: `mov x8, #SYS_rt_sigreturn; svc #0`. We can't move this to Rust.
- `sigsetjmp`/`siglongjmp` (`src/setjmp/aarch64/{setjmp,longjmp}.s`) — keep as `.s`. Rust naked functions could do it but the existing 30-line files are bulletproof.
- `sigprocmask` → `rt_sigprocmask`. `sigsuspend` → `rt_sigsuspend`. We use the `rt_*` variants exclusively (the older `sigprocmask` syscall doesn't exist on aarch64).
- `signalfd` is direct.

### 6.11 `mytilus-fs`, `mytilus-fcntl`, `mytilus-unistd`, `mytilus-mman`
Mostly thin syscall wrappers + a few non-trivial ones:
- **`stat` family** all funnel through `SYS_statx` (kernel ≥ 4.11). We map `struct statx` → `struct stat`. No `stat64`/`fstat64`/`lstat64`/`fstatat64` exist on aarch64 — only the modern names. We expose them.
- **`open` / `openat` / `creat`** — `creat` is `openat(AT_FDCWD, path, O_WRONLY|O_CREAT|O_TRUNC, mode)`. `open` is `openat(AT_FDCWD, ...)`.
- **`lseek`** uses `SYS_lseek` directly (it's a 64-bit syscall on aarch64; no `_llseek`).
- **`getdents`** uses `SYS_getdents64` directly.
- **`mmap`** uses `SYS_mmap` directly (no `mmap2` on aarch64). Page size from `AT_PAGESZ` (commonly 4K, but ARM systems also run 16K and 64K — we must respect `AT_PAGESZ`; never hardcode 4096).
- `realpath`, `glob`, `fnmatch`, `wordexp` — pure Rust ports.

### 6.12 `mytilus-process`
- **`fork`** — implemented via `clone(SIGCHLD)` (or `clone3` with `CLONE_CLEAR_SIGHAND`-aware flags). Post-fork in the child, run the registered atfork hooks; reset the malloc lock; fix up the `pthread_self()->tid`.
- **`vfork`** stays in **assembly** (`src/process/aarch64/vfork.s`). Reason: between `clone(CLONE_VM|CLONE_VFORK|SIGCHLD)` returning and `execve` running in the child, the parent's stack is shared and any non-leaf call corrupts state.
- **`posix_spawn`** — direct port; uses `vfork`+`execve` internally.
- **`execve`/`execvp`/`execle`/...` family.
- **`waitpid`/`wait4`/`waitid`** — via `SYS_waitid` for the modern path.

### 6.13 `mytilus-net` — sockets and resolver
- Socket calls: aarch64 has direct `SYS_socket`, `SYS_bind`, `SYS_connect`, `SYS_accept4`, `SYS_sendto`, `SYS_recvfrom`, `SYS_sendmsg`, `SYS_recvmsg`, `SYS_setsockopt`, `SYS_getsockopt`, `SYS_shutdown`, `SYS_socketpair`, `SYS_listen`. **No `socketcall`** — drop entirely.
- Resolver: pure Rust port of `src/network/lookup_*`. `/etc/resolv.conf`, `/etc/hosts`, `/etc/services`. DNS over UDP with TCP fallback. DNSSEC is not in scope (musl doesn't do it either).
- `getaddrinfo`/`getnameinfo` faithful port.
- `if_*`, `inet_*`, `htons`/`ntohs` — trivial; on aarch64 byteswap is `rev16`/`rev32`/`rev64`.

### 6.14 `mytilus-locale` & multibyte
- The locale subsystem is mostly "C / C.UTF-8 / POSIX" with a message-catalog mechanism (`MUSL_LOCPATH`).
- UTF-8 handling: faithful port of `src/multibyte/`. The Rust port can lean on `core::str` validation in the *internal* paths but the public symbols stay byte-oriented (mbrtowc et al.).
- iconv: we re-port. The codeset list is hardcoded; UTF-8/UTF-16/UTF-32/legacy 8-bit codepages.

### 6.15 `mytilus-ldso` — the dynamic linker
**The single biggest piece of work after pthreads.** Approx. 3–4k LOC in upstream, all in `ldso/dynlink.c` and `ldso/dlstart.c`.

- **`dlstart`** (the entry point of `ld-musl-aarch64.so.1`) **must remain partly assembly**. The first thing the kernel does is jump to it with the auxiliary vector on the stack and **no relocations applied to ld-musl itself yet**. So `dlstart` runs in a state where:
  - it can't call any function that uses GOT indirection,
  - it can't read any data with a relocation applied,
  - it has no stack frame conventions.

  Upstream's solution is a tiny PIC bootstrap that reads `_DYNAMIC`, walks the relocation table, applies its own `R_AARCH64_RELATIVE` relocs, and *then* tail-calls into the C `__dls2`. We mirror this. The entry stub stays as `.s`. From `__dls2` onward we're in Rust.
- **Phases:**
  1. `__dls2` — relocate ourselves (we now have a working GOT).
  2. `__dls2b` — set up the initial TLS for ld-musl.
  3. `__dls3` — load the executable's `PT_INTERP`-mate (us), parse `PT_DYNAMIC` of the executable, load each `DT_NEEDED`, apply relocations in dependency order, run init_arrays, transfer control to `_start` of the executable.
- **Relocation types we must handle on AArch64:**
  - `R_AARCH64_NONE`
  - `R_AARCH64_ABS64`, `R_AARCH64_ABS32`, `R_AARCH64_ABS16`
  - `R_AARCH64_PREL64`, `R_AARCH64_PREL32`
  - `R_AARCH64_GLOB_DAT`
  - `R_AARCH64_JUMP_SLOT`
  - `R_AARCH64_RELATIVE`
  - `R_AARCH64_COPY`
  - `R_AARCH64_TLS_DTPMOD64`, `R_AARCH64_TLS_DTPREL64`, `R_AARCH64_TLS_TPREL64`
  - `R_AARCH64_TLSDESC`
  - `R_AARCH64_IRELATIVE`
- **Lazy binding:** `_dl_runtime_resolve` — keep as `.s`; the calling convention preserves all argument registers and only clobbers `x16`, `x17`, `lr`. Must be hand-tuned.
- **`dlopen`/`dlsym`/`dlclose`/`dladdr`/`dlinfo`** — Rust port.
- **`dl_iterate_phdr`** — Rust port.
- We **drop FDPIC** entirely (FDPIC is not an aarch64 thing).
- We **drop `LD_PRELOAD` validation hacks for setuid** that exist for older non-aarch64 paths; aarch64 has had `AT_SECURE` since forever.

### 6.16 `mytilus-crypt` & `mytilus-prng`
- `crypt(3)`: support `$1$` (md5), `$5$` (sha256), `$6$` (sha512), `$2a$`/`$2b$` (bcrypt) — same set as upstream. Rust ports of the hashes (we ship our own — no `sha2` crate).
- DES `crypt` for legacy: keep but document as deprecated.
- `getrandom` is the bottom of the entropy stack. `rand`/`rand_r`/`random`/`erand48`/`drand48` are user-facing.

### 6.17 `mytilus-regex`, `mytilus-search`, `mytilus-aio`, `mytilus-ipc`
Faithful ports. The TRE regex engine in upstream is ~3k LOC, well-bounded, port to safe Rust. Search trees (`tsearch`) port cleanly. AIO is implemented in user space (helper-thread pool); SysV/POSIX IPC is mostly syscall wrappers.

---

## 7. Things we **delete** vs. upstream

Listing these out so the reduction is concrete:

| Removed | Reason |
|---|---|
| `arch/{arm,i386,m68k,microblaze,mips,mips64,mipsn32,or1k,powerpc,powerpc64,riscv32,riscv64,s390x,sh,x32,x86_64,loongarch64,generic}` | Single-target. |
| `compat/time32/` | 64-bit `time_t` only. |
| Every `*64` variant (`stat64`, `fstat64`, `lseek64`, `mmap64`, `pread64` userspace shim, `truncate64`, `ftruncate64`, `getrlimit64`, `setrlimit64`, `prlimit64` shim, `fopen64`, `tmpfile64`, `creat64`, `aio_*64`, `lockf64`, `statvfs64`, `fstatvfs64`, `nftw64`, `ftw64`, `glob64`, `getdents64` userspace shim, `posix_fadvise64`, `posix_fallocate64`, `mmap64`, `pwrite64`, `readdir64`, `seekdir64`, `telldir64`) | On a 64-bit-only libc, the unsuffixed name and the `*64` suffix would alias. We make `stat == stat64` etc. via weak aliases — same as upstream does on 64-bit, but we never *generate* the `*32`. |
| `__SYSCALL_LL_E` / `__SYSCALL_LL_O` syscall pair-packing | Unneeded on 64-bit. |
| `socketcall` indirection | aarch64 has direct socket syscalls. |
| `mmap2` / `_llseek` / `fcntl64` / `fstatat64` / `truncate64` syscall paths | Don't exist on aarch64. |
| FDPIC support (`fdpic_crt.h`) | Not aarch64. |
| Big-endian byte-swap shims | aarch64 LE only. |
| Soft-float math | AArch64 has FPU. |

This shrinks the line count by roughly **a third to a half** before any Rust syntax savings.

---

## 8. Build, install, and packaging

- **`make all`** runs `cargo build --release` on the umbrella crate plus the dynamic-linker crate, then assembles:
  - `lib/libc.so` — the dynamic library, with `DT_SONAME = libc.so` and a symlink `ld-musl-aarch64.so.1 -> libc.so` (this is musl's quirk: ld-musl *is* libc).
  - `lib/libc.a` — static archive (a partial-link object covering all subcrates).
  - `lib/crt1.o`, `Scrt1.o`, `rcrt1.o`, `crti.o`, `crtn.o`, `gcrt1.o`.
  - `include/` — headers, mostly verbatim from upstream's `include/` plus a single `bits/` (the aarch64 set).
- **`make install` `DESTDIR=/path PREFIX=/usr`** mirrors upstream's `Makefile` install rules so distros don't need to learn anything new.
- **`musl-gcc`-style wrapper:** ship `aarch64-linux-muslrs-gcc` and `aarch64-linux-muslrs-ld` wrappers that pass `-B`, `-isystem`, and `--dynamic-linker=/lib/ld-musl-aarch64.so.1`.
- **Cargo target spec:** publish `aarch64-unknown-linux-mytilus.json` so Rust users can `cargo build --target=aarch64-unknown-linux-mytilus`.

---

## 9. Testing

| Layer | Approach |
|---|---|
| Unit tests | Each non-`unsafe` crate has `#[cfg(test)]` tests run on the host (`x86_64-unknown-linux-gnu` is fine) under `std`. The crates are no-std at runtime but `dev-dependencies` may pull `std` for tests only. |
| Conformance | Run **libc-test** (musl's own test suite, http://nsz.repo.hu/git/?p=libc-test) against the built `.so` on aarch64 hardware (or QEMU user-mode). Goal: zero regressions vs. upstream musl 1.2.6 on the same system. |
| Glibc-style suites | Run a curated subset of glibc's test suite that is portable. |
| Real binaries | Static-link `busybox`, `bash`, `coreutils`, `nginx`, and `python3` against `libc.a`; dynamic-link them against `libc.so`. CI runs these binaries through their own test suites. |
| Fuzzing | `cargo-fuzz` (host-side) on `printf`, `scanf`, `strtod`, `getaddrinfo`'s parser, `glob`, `fnmatch`, `regex`, `iconv`. Differential fuzzing against upstream musl. |
| Differential allocator | Run `mallocng`'s upstream stress tests against our port; bit-for-bit metadata equivalence is not required, but observable behavior (sizes returned, alignment, fragmentation curves) must match within tolerance. |
| Symbol diff | CI: `nm -D --defined-only` on our `libc.so` must equal upstream's. Any addition or removal fails CI without an explicit allowlist. |
| ABI diff | `abidiff` (libabigail) against upstream `libc.so` for aarch64. Public struct layouts must match. |

---

## 10. Phasing — what to build, in what order

The dependency graph forces a particular order. Skipping it leads to a half-built libc that can't even link a hello-world.

1. **Foundation** (week 1–2):
   - `mytilus-sys` (syscalls, ctypes, errno classification)
   - `mytilus-internal` (atomics, futex helpers, lock primitives, doubly-linked list)
   - `mytilus-errno`
   - `mytilus-string` (memcpy/memset asm pulled in, rest in Rust)
   - Custom target spec, `build-std`, CI green on `cargo build`.
2. **Bootstrap** (week 2–3):
   - `mytilus-startup`: `crt1`/`crti`/`crtn`/`Scrt1`/`__libc_start_main`.
   - `mytilus-mman`: `mmap`/`munmap`/`mprotect`/`madvise`.
   - `mytilus-malloc`: `lite_malloc` first, then port `mallocng`.
   - Goal: a `hello-world.c` that links statically against `libc.a` and prints "hello\n" via `write(1, ...)` → then via a stripped-down `puts`.
3. **stdio + stdlib** (week 3–5):
   - `mytilus-stdio` (FILE, write/read paths, vfprintf, vfscanf).
   - `mytilus-stdlib` (strtol family, env, qsort, exit, atexit).
   - Goal: a `printf("%g %s\n", 3.14, name)` works.
4. **Time + math** (week 5–7):
   - `mytilus-time`, `mytilus-math` — these are mostly leaf nodes.
5. **Threads** (week 7–10):
   - `mytilus-thread` (pthreads, TLS init in static-link case).
   - `mytilus-signal`.
   - Goal: a static binary with 8 threads doing futex-mediated work.
6. **Filesystem & process** (week 10–12):
   - `mytilus-fs`, `mytilus-fcntl`, `mytilus-unistd`, `mytilus-process`.
7. **Network** (week 12–14):
   - `mytilus-net`.
   - Goal: a static `nc`-like binary works.
8. **Dynamic linker** (week 14–18):
   - `mytilus-ldso`.
   - This is sequenced *late* because by now every dependency it has is debugged statically.
   - Goal: build `libc.so` and run a dynamically-linked busybox.
9. **The long tail** (week 18+):
   - `mytilus-locale`, `mytilus-regex`, `mytilus-search`, `mytilus-crypt`, `mytilus-aio`, `mytilus-ipc`, `mytilus-prng`, `mytilus-passwd`.
10. **Hardening** (rolling):
    - libc-test, fuzzing, ABI diff, real-binary CI.

A realistic single-developer estimate is **9–14 months** of full-time work to reach "boots a userland". A small team (3 engineers) could compress to **6 months** if the dynamic linker and pthreads are owned by experienced systems engineers.

---

## 11. Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| Rust nightly breaks `naked_functions` or `asm_const` semantics | Medium | Pin toolchain; have an asm-file fallback for every naked function; subscribe to release notes. |
| `compiler_builtins` produces a `memcpy` that overrides ours | High | Build with `-Z build-std-features` *not* including `compiler-builtins-mem`, and CI-check that `nm libc.a \| grep memcpy` resolves to our object. |
| TLSDESC subtleties on early `dlopen` | High | Mirror upstream exactly; do not "improve". Have a focused test that `dlopen`s a lib with `__thread` vars and reads them from multiple threads. |
| Allocator metadata corruption from `fork` | Medium | Adopt mallocng's atfork-safe design as-is. Don't redesign. |
| Subtle `errno` clobbering in `__syscall_cp` cancellation path | High | Keep the `.s` file. Don't port to Rust naked. Period. |
| Performance regression on `memcpy` if we port to Rust | Medium | Don't port. Keep the `.S`. |
| ABI drift (a struct field changes alignment) | Medium | `abidiff` in CI. `assert_eq!(size_of::<stat>(), 128)` style static asserts in every public struct module. |
| Symbol drift (we accidentally export a Rust mangled symbol) | Low | Linker version script + CI symbol diff. |
| Panic in libc internals reaches user code | Medium | `panic = abort` workspace-wide; custom `#[panic_handler]` calls `__libc_fatal`; CI lints for `unwrap()`/`expect()` outside test code. |
| Header rot — Rust port drifts from header constants | Medium | Single source of truth: headers are checked-in C; constants used in Rust come from a build-script that parses them with `bindgen` (or a hand-rolled equivalent — `bindgen` itself uses libclang; pulling that in for build-time is acceptable). |

---

## 12. Open questions to resolve before code starts

1. **Do we ship a `musl-gcc`-style wrapper or just a Rust target spec?** If both, who owns the wrapper script.
2. **`bindgen` at build time — yes or no?** It's the simplest way to keep header constants in sync. Cost: it's a host-time-only dep, not a runtime dep, but it pulls in libclang.
3. **Stable Rust path?** Several features we need are nightly-only. Do we hold for stabilization (slow) or commit to nightly (faster, more fragile)?
4. **Licensing.** Upstream musl is MIT. We must keep that license on every ported file and add a NOTICE for files we author. Make sure the project license is `MIT` and include a `COPYRIGHT` mirroring upstream's per-file attributions.
5. **Naming.** "mytilus" is a working name. If we ever want a `pkg-config` name and `DT_SONAME` distinct from upstream we need to choose now (proposal: keep `libc.so` and `ld-musl-aarch64.so.1` for ABI compat, but ship `pkgconfig/mytilus.pc`).
6. **Page size.** ARM systems run 4K, 16K, and 64K pages. Read `AT_PAGESZ` everywhere; never hardcode. Have a CI matrix entry that runs on a 16K-page kernel.
7. **HW capabilities.** `AT_HWCAP` / `AT_HWCAP2` exposes things like `HWCAP_CRC32`, `HWCAP_AES`, `HWCAP_SHA1`, `HWCAP_SHA2`, `HWCAP_ASIMD`, `HWCAP2_SVE2`. Decide which we use as fast paths and which we ignore.

---

## 13. Summary of crate dependency graph

```
                         mytilus-sys
                            │
        ┌───────────────────┼──────────────────┐
        │                   │                  │
   mytilus-internal       mytilus-errno         mytilus-string (asm)
        │                   │                  │
        └─────┬─────────────┘                  │
              │                                │
         mytilus-mman ──── mytilus-malloc            │
              │              │                 │
              └──────┬───────┘                 │
                     │                         │
                mytilus-startup  ────────────────►│
                     │
   ┌────────┬────────┼────────┬────────┬───────┬────────┐
   │        │        │        │        │       │        │
mytilus-stdio mytilus-time mytilus-math mytilus-fs mytilus-fcntl mytilus-unistd mytilus-stdlib
   │        │                  │        │       │        │
   └────────┴──────────┬───────┴────────┴───────┘        │
                       │                                 │
                  mytilus-signal                            │
                       │                                 │
                  mytilus-thread ◄────────────────────────  │
                       │
                  mytilus-process
                       │
                  mytilus-net, mytilus-ipc, mytilus-aio, mytilus-locale,
                  mytilus-regex, mytilus-search, mytilus-crypt, mytilus-prng,
                  mytilus-passwd
                       │
                  mytilus-ldso (depends only on mytilus-sys, mytilus-internal,
                              mytilus-mman, mytilus-string — *not* on malloc)
                       │
                  mytilus (umbrella re-export)
```

The two firewalls:
- **`mytilus-ldso` does not depend on `mytilus-malloc`** (it has its own bump allocator).
- **No crate depends on `std` or `alloc` at runtime** (only at test time).

These are encoded as `[dependencies]` constraints and as CI lints (`cargo deny`-style) so they cannot regress silently.
