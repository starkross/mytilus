//! `mytilus-errno` — `__errno_location`, `strerror`, error-message table.
//!
//! Mirrors `src/errno/` upstream (`__errno_location.c`, `strerror.c`).
//! `strerror_r` and `perror` live in their respective string/stdio crates,
//! same as upstream.
//!
//! Target: aarch64-unknown-linux, 64-bit only.

#![no_std]
#![feature(thread_local)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use core::ffi::CStr;

use mytilus_sys::ctypes::{c_char, c_int, c_void};
use mytilus_sys::errno_raw as e;
// Re-export the constants so `mytilus_errno::EAGAIN` works without pulling in
// `mytilus_sys` directly.
pub use mytilus_sys::errno_raw::*;

/// Opaque `locale_t` placeholder. Will be retyped to `mytilus_locale::Locale`
/// once that crate lands; the C ABI signature is unchanged either way.
pub type locale_t = *mut c_void;

// ---------------------------------------------------------------------------
// __errno_location
// ---------------------------------------------------------------------------
//
// Upstream:
//
//     int *__errno_location(void) { return &__pthread_self()->errno_val; }
//
// We don't have `mytilus-thread` / `Pthread` yet, so the slot lives in this
// crate's TLS for now. Once `__pthread_self` exists, the body of this fn
// should change to read the field on the pthread struct, and this stand-in
// `ERRNO_VAL` should be deleted.

#[thread_local]
static mut ERRNO_VAL: c_int = 0;

#[no_mangle]
pub extern "C" fn __errno_location() -> *mut c_int {
    core::ptr::addr_of_mut!(ERRNO_VAL)
}

// glibc-internal alias kept for compatibility with binaries that import the
// triple-underscore name. Upstream: `weak_alias(__errno_location, ___errno_location)`.
#[no_mangle]
pub extern "C" fn ___errno_location() -> *mut c_int {
    __errno_location()
}

// ---------------------------------------------------------------------------
// strerror / __strerror_l / strerror_l
// ---------------------------------------------------------------------------

/// Returns a static C string describing `err`. Out-of-range or unmapped values
/// yield the catch-all `"No error information"` — same as upstream.
///
/// Used internally by `strerror` here, by `strerror_r` in `mytilus-string`,
/// and by `perror` in `mytilus-stdio`.
pub fn strerror_str(err: c_int) -> &'static CStr {
    let idx = if err < 0 || (err as usize) >= MESSAGES.len() {
        0
    } else {
        err as usize
    };
    MESSAGES[idx]
}

#[no_mangle]
pub extern "C" fn __strerror_l(err: c_int, _loc: locale_t) -> *mut c_char {
    // Locale-aware translation is a no-op until `mytilus-locale` provides
    // `LCTRANS`; the C/POSIX locale returns the raw string verbatim, which is
    // what we do here.
    strerror_str(err).as_ptr() as *mut c_char
}

#[no_mangle]
pub extern "C" fn strerror_l(err: c_int, loc: locale_t) -> *mut c_char {
    __strerror_l(err, loc)
}

#[no_mangle]
pub extern "C" fn strerror(err: c_int) -> *mut c_char {
    // CURRENT_LOCALE upstream; null is fine since __strerror_l ignores it.
    __strerror_l(err, core::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// Message table — mirrors src/errno/__strerror.h.
// ---------------------------------------------------------------------------
//
// Sized to `max-errno-with-message + 1`. Slots without a dedicated message
// fall back to entry 0 (the catch-all). aarch64 doesn't need the MIPS
// EDQUOT=1133 remap.

const UNK: &CStr = c"No error information";

const MESSAGES: [&CStr; 132] = {
    let mut m: [&CStr; 132] = [UNK; 132];
    m[e::EILSEQ as usize] = c"Illegal byte sequence";
    m[e::EDOM as usize] = c"Domain error";
    m[e::ERANGE as usize] = c"Result not representable";
    m[e::ENOTTY as usize] = c"Not a tty";
    m[e::EACCES as usize] = c"Permission denied";
    m[e::EPERM as usize] = c"Operation not permitted";
    m[e::ENOENT as usize] = c"No such file or directory";
    m[e::ESRCH as usize] = c"No such process";
    m[e::EEXIST as usize] = c"File exists";
    m[e::EOVERFLOW as usize] = c"Value too large for data type";
    m[e::ENOSPC as usize] = c"No space left on device";
    m[e::ENOMEM as usize] = c"Out of memory";
    m[e::EBUSY as usize] = c"Resource busy";
    m[e::EINTR as usize] = c"Interrupted system call";
    m[e::EAGAIN as usize] = c"Resource temporarily unavailable";
    m[e::ESPIPE as usize] = c"Invalid seek";
    m[e::EXDEV as usize] = c"Cross-device link";
    m[e::EROFS as usize] = c"Read-only file system";
    m[e::ENOTEMPTY as usize] = c"Directory not empty";
    m[e::ECONNRESET as usize] = c"Connection reset by peer";
    m[e::ETIMEDOUT as usize] = c"Operation timed out";
    m[e::ECONNREFUSED as usize] = c"Connection refused";
    m[e::EHOSTDOWN as usize] = c"Host is down";
    m[e::EHOSTUNREACH as usize] = c"Host is unreachable";
    m[e::EADDRINUSE as usize] = c"Address in use";
    m[e::EPIPE as usize] = c"Broken pipe";
    m[e::EIO as usize] = c"I/O error";
    m[e::ENXIO as usize] = c"No such device or address";
    m[e::ENOTBLK as usize] = c"Block device required";
    m[e::ENODEV as usize] = c"No such device";
    m[e::ENOTDIR as usize] = c"Not a directory";
    m[e::EISDIR as usize] = c"Is a directory";
    m[e::ETXTBSY as usize] = c"Text file busy";
    m[e::ENOEXEC as usize] = c"Exec format error";
    m[e::EINVAL as usize] = c"Invalid argument";
    m[e::E2BIG as usize] = c"Argument list too long";
    m[e::ELOOP as usize] = c"Symbolic link loop";
    m[e::ENAMETOOLONG as usize] = c"Filename too long";
    m[e::ENFILE as usize] = c"Too many open files in system";
    m[e::EMFILE as usize] = c"No file descriptors available";
    m[e::EBADF as usize] = c"Bad file descriptor";
    m[e::ECHILD as usize] = c"No child process";
    m[e::EFAULT as usize] = c"Bad address";
    m[e::EFBIG as usize] = c"File too large";
    m[e::EMLINK as usize] = c"Too many links";
    m[e::ENOLCK as usize] = c"No locks available";
    m[e::EDEADLK as usize] = c"Resource deadlock would occur";
    m[e::ENOTRECOVERABLE as usize] = c"State not recoverable";
    m[e::EOWNERDEAD as usize] = c"Previous owner died";
    m[e::ECANCELED as usize] = c"Operation canceled";
    m[e::ENOSYS as usize] = c"Function not implemented";
    m[e::ENOMSG as usize] = c"No message of desired type";
    m[e::EIDRM as usize] = c"Identifier removed";
    m[e::ENOSTR as usize] = c"Device not a stream";
    m[e::ENODATA as usize] = c"No data available";
    m[e::ETIME as usize] = c"Device timeout";
    m[e::ENOSR as usize] = c"Out of streams resources";
    m[e::ENOLINK as usize] = c"Link has been severed";
    m[e::EPROTO as usize] = c"Protocol error";
    m[e::EBADMSG as usize] = c"Bad message";
    m[e::EBADFD as usize] = c"File descriptor in bad state";
    m[e::ENOTSOCK as usize] = c"Not a socket";
    m[e::EDESTADDRREQ as usize] = c"Destination address required";
    m[e::EMSGSIZE as usize] = c"Message too large";
    m[e::EPROTOTYPE as usize] = c"Protocol wrong type for socket";
    m[e::ENOPROTOOPT as usize] = c"Protocol not available";
    m[e::EPROTONOSUPPORT as usize] = c"Protocol not supported";
    m[e::ESOCKTNOSUPPORT as usize] = c"Socket type not supported";
    m[e::ENOTSUP as usize] = c"Not supported";
    m[e::EPFNOSUPPORT as usize] = c"Protocol family not supported";
    m[e::EAFNOSUPPORT as usize] = c"Address family not supported by protocol";
    m[e::EADDRNOTAVAIL as usize] = c"Address not available";
    m[e::ENETDOWN as usize] = c"Network is down";
    m[e::ENETUNREACH as usize] = c"Network unreachable";
    m[e::ENETRESET as usize] = c"Connection reset by network";
    m[e::ECONNABORTED as usize] = c"Connection aborted";
    m[e::ENOBUFS as usize] = c"No buffer space available";
    m[e::EISCONN as usize] = c"Socket is connected";
    m[e::ENOTCONN as usize] = c"Socket not connected";
    m[e::ESHUTDOWN as usize] = c"Cannot send after socket shutdown";
    m[e::EALREADY as usize] = c"Operation already in progress";
    m[e::EINPROGRESS as usize] = c"Operation in progress";
    m[e::ESTALE as usize] = c"Stale file handle";
    m[e::EUCLEAN as usize] = c"Data consistency error";
    m[e::ENAVAIL as usize] = c"Resource not available";
    m[e::EREMOTEIO as usize] = c"Remote I/O error";
    m[e::EDQUOT as usize] = c"Quota exceeded";
    m[e::ENOMEDIUM as usize] = c"No medium found";
    m[e::EMEDIUMTYPE as usize] = c"Wrong medium type";
    m[e::EMULTIHOP as usize] = c"Multihop attempted";
    m[e::ENOKEY as usize] = c"Required key not available";
    m[e::EKEYEXPIRED as usize] = c"Key has expired";
    m[e::EKEYREVOKED as usize] = c"Key has been revoked";
    m[e::EKEYREJECTED as usize] = c"Key was rejected by service";
    m
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_message() {
        assert_eq!(
            strerror_str(EAGAIN).to_bytes(),
            b"Resource temporarily unavailable"
        );
    }

    #[test]
    fn unmapped_falls_back() {
        // ECHRNG (44) has no entry in __strerror.h.
        assert_eq!(strerror_str(ECHRNG).to_bytes(), b"No error information");
    }

    #[test]
    fn out_of_range_falls_back() {
        assert_eq!(strerror_str(9999).to_bytes(), b"No error information");
        assert_eq!(strerror_str(-1).to_bytes(), b"No error information");
    }

    #[test]
    fn aliases_resolve() {
        assert_eq!(EWOULDBLOCK, EAGAIN);
        assert_eq!(ENOTSUP, EOPNOTSUPP);
        assert_eq!(EDEADLOCK, EDEADLK);
    }

    #[test]
    fn errno_location_round_trip() {
        let p = __errno_location();
        // SAFETY: per-thread slot; this test runs on a single thread and the
        // pointer is exclusive to it for the duration of the test.
        unsafe {
            *p = EINVAL;
            assert_eq!(*__errno_location(), EINVAL);
            *p = 0;
        }
    }
}
