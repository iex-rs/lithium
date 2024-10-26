/// An unwinding backend.
///
/// Unwinding is a mechanism of forcefully "returning" through multiple call frames, called
/// *throwing*, up until a special call frame, called *interceptor*. This roughly corresponds to the
/// `resume_unwind`/`catch_unwind` pair on Rust and `throw`/`catch` pair on C++.
///
/// It's crucial that unwinding doesn't require (source-level) cooperation from the intermediate
/// call frames.
///
/// Backends are supposed to treat exception objects as opaque, except for
/// [`Backend::ExceptionHeader`]. This also means that exception objects are untyped from the
/// backend's point of view.
///
/// # Safety
///
/// Implementations of this trait must ensure that when an exception pointer is thrown, unwinding
/// proceeds to the closest (most nested) `intercept` frame and that `intercept` returns this exact
/// pointer (including provenance). The implementation may modify the header arbitrarily during
/// unwinding, but modifying any other data from the same allocation is forbidden.
///
/// Exceptions may not be ignored or caught twice.
///
/// Note that several exceptions can co-exist at once, even in a single thread. This can happen if
/// a destructor that uses exceptions (without letting them escape past `drop`) is invoked during
/// unwinding from another exception. This can be nested arbitrarily. In this context, the order of
/// catching must be in the reverse order of throwing.
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
    /// The first requirement is that `ex` is a unique pointer to an exception object, cast to
    /// `*mut Self::ExceptionHeader`.
    ///
    /// Secondly, it is important that intermediate call frames don't preclude unwinding from
    /// happening soundly. For example, [`catch_unwind`](std::panic::catch_unwind) can safely catch
    /// panics and may start catching foreign exceptions soon, both of which can confuse the user of
    /// this trait.
    ///
    /// For this reason, the caller must ensure no intermediate frames can affect unwinding. This
    /// includes not passing throwing callbacks to foreign crates, but also not using `throw` in own
    /// code that might `intercept` an exception without cooperation with the throwing side.
    unsafe fn throw(ex: *mut Self::ExceptionHeader) -> !;

    /// Catch an exception.
    ///
    /// This function returns `Ok` if the function returns normally, or `Err` if it throws (and the
    /// thrown exception is not caught by a nested interceptor). If `Err` is returned, the pointer
    /// must match what was thrown, including provenance.
    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut Self::ExceptionHeader>;
}

#[cfg(backend = "itanium")]
#[path = "itanium.rs"]
mod imp;

#[cfg(backend = "panic")]
#[path = "panic.rs"]
mod imp;

pub(crate) use imp::ActiveBackend;
