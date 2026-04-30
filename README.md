# mytilus

[![CI](https://github.com/starkross/mytilus/actions/workflows/ci.yml/badge.svg)](https://github.com/starkross/mytilus/actions/workflows/ci.yml)

A reimplementation of [musl libc](http://www.musl-libc.org/) (1.2.6) in Rust.

> **Status: experimental — pre-alpha skeleton. Do not use.**
>
> This repository currently contains only the workspace layout and empty crate
> scaffolds. There is no working libc yet. Even when filled in, mytilus is
> intentionally a **single-target** project: `aarch64-unknown-linux`, 64-bit
> little-endian only. It will never support x86_64, 32-bit ARM, RISC-V, MIPS,
> PowerPC, s390x, big-endian aarch64, or any non-Linux OS. The musl `arch/`
> tree (18 architectures) and all 32-bit / `time32` / `off64`-shim code paths
> are dropped on purpose.
>
> If you need a portable Rust libc, look at
> [`relibc`](https://github.com/redox-os/relibc) or
> [`c-ward`](https://github.com/sunfishcode/c-ward) instead. Do not depend on
> mytilus's symbol set, ABI, or even existence being stable; pin to a commit
> if you must.

**Scope:** `aarch64-unknown-linux` only, 64-bit only. See `PLAN.md` for the full
design and `arch/`-pruning rationale.

## Layout

```
crates/             # 28 no_std crates that compose the libc
headers/            # C headers shipped with the libc (one bits/ — aarch64)
linker/             # version scripts, dynamic.list
tools/              # codegen tools (mksyscalls, etc.)
aarch64-unknown-linux-mytilus.json   # custom target spec
Taskfile.yml        # task runner (Taskfile, not Make)
```

## Building

```sh
task build           # cross-compile libc.{so,a} for aarch64
task check:host      # cargo check on the host (for IDE)
task test            # run host-side unit tests
task ci              # the full CI gauntlet locally
task --list-all      # everything
```

You need:

- the pinned nightly Rust toolchain (auto-installed via `rust-toolchain.toml`)
- [Taskfile](https://taskfile.dev/installation/)
- on macOS: a working `rust-lld` (ships with rustup)

## Why no `std` and no crates.io

We *are* the libc. Pulling in `std` or any external crate would be circular.
The dependency graph is: `core` + `compiler_builtins` + us. That is enforced
in CI.

## License

MIT. See `LICENSE`. Per-file attributions for ports of upstream musl code
mirror upstream's `COPYRIGHT`.
