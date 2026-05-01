//! `mytilus-string` — `mem*` / `str*` / `wmem*` / `wcs*` implementations.
//!
//! Phase 1 ports the four `mem*` symbols (`memcpy`, `memmove`, `memset`,
//! `memcmp`). Compiler-builtins-mem is explicitly disabled in the workspace,
//! so these symbols MUST be provided by us before any final binary can link.
//!
//! These implementations are deliberately simple byte loops. They are
//! correct and pass `cargo test` cleanly, but they are not optimized.
//! Upstream musl ships hand-written `aarch64/memcpy.S` / `memset.S` and
//! relies on its complex generic C versions on other arches; PLAN.md
//! commits to the same approach for the perf path. The asm replacements
//! will land in a follow-up.
//!
//! Symbol gating: `#[cfg_attr(target_env = "musl", no_mangle)]` keeps the C names off
//! the symbol table when the test binary is linked against the host libc —
//! otherwise our `memcpy` collides with libsystem's. The cross target (and
//! plain `cargo build`) gets the unmangled names.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

mod mem;
mod str_fns;
mod str_search;

pub use mem::*;
pub use str_fns::*;
pub use str_search::*;
