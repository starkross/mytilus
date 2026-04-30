//! `mytilus-signal` — `sigaction`, `signal`, sigset, signal-restore
//! trampoline.
//!
//! Phase 1 ports just the **sigset_t bit-manipulators** from `src/signal/`
//! upstream: `sigemptyset`, `sigfillset`, `sigaddset`, `sigdelset`,
//! `sigismember`, `sigorset`, `sigandset`, `sigisemptyset`. Plus the
//! `sigset_t` struct definition.
//!
//! All eight functions are pure — no syscalls, no allocator. They establish
//! the canonical `sigset_t` that `sigaction`, `sigprocmask`, `pthread_sigmask`,
//! `sigtimedwait`, etc. will all consume.
//!
//! Deferred to later phases: signal-handler installation (`sigaction`,
//! `signal`, `bsd_signal`), masking (`sigprocmask`, `pthread_sigmask`),
//! delivery (`raise`, `kill`, `tkill`, `tgkill`), the `restore` trampoline,
//! `siginfo_t`, real-time signal support, `sigsuspend`, `sigwait*`. Those
//! all need real syscalls plus the cancellation/thread machinery.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod sigset;

pub use sigset::*;
