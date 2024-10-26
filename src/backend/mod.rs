/// An unwinding backend.
///
/// Unwinding is a mechanism of forcefully "returning" through multiple call frames, called
/// *throwing*, up until a special call frame, called *interceptor*. This roughly corresponds to the
/// `resume_unwind`/`catch_unwind` pair on Rust and `throw`/`catch` pair on C++.
///
/// It's crucial that unwinding doesn't require (source-level) cooperation from the intermediate
/// call frames.
///
/// However, it is also important that the intermediate call frames don't preclude unwinding from
/// happening soundly. In particular, [`catch_unwind`](std::panic::catch_unwind) can safely catch
/// panics, and may start catching foreign exceptions soon, both of which can confuse the backend.
/// For this reason, there are safety requirements beyond type equality.
///
/// Backends are supposed to treat exception objects as opaque, except for
/// [`Backend::ExceptionHeader`]. This also means that exception objects are untyped from the
/// backend's point of view.
pub unsafe trait Backend {
    /// An exception header.
    ///
    /// Allocated exceptions, as stored in the [`Exception`] type, will immediately begin with this
    /// header. This allows that exception pointers to be used with ABIs that require exceptions to
    /// start with custom information, like Itanium EH ABI.
    type ExceptionHeader;

    /// Create a new exception header.
    ///
    /// This will be called whenever a new exception needs to be allocated.
    fn new_header() -> Self::ExceptionHeader;

    /// Throw an exception.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `ex` is a unique pointer to an exception object, cast to `ExceptionHeader`.
    /// - The exception cannot be caught by the system runtime, e.g. [`std::panic::catch_unwind`] or
    ///    [`std::thread::spawn`].
    unsafe fn throw(ex: *mut Self::ExceptionHeader) -> !;

    /// Catch an exception.
    ///
    /// This function returns `Ok` if the function returns normally, or `Err` if it throws (and the
    /// thrown exception is not caught by a nested interceptor). If `Err` is returned, the pointer
    /// must match what was thrown, including provenance.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `func` only throws exceptions of
    unsafe fn intercept<Func: FnOnce() -> R, R>(
        func: Func,
    ) -> Result<R, *mut Self::ExceptionHeader>;
}

#[cfg(backend = "itanium")]
#[path = "itanium.rs"]
mod imp;

#[cfg(backend = "panic")]
#[path = "panic.rs"]
mod imp;

pub use imp::ActiveBackend;
