use super::{
    backend::{ActiveBackend, Backend},
    exceptions::{pop, replace_last, Exception},
};

/// Not-quite-caught exception.
///
/// This type is returned by [`intercept`] when an exception is caught. Exception handling is not
/// yet done at that point: it's akin to entering a `catch` clause in C++.
///
/// At this point, you can either drop the handle, which halts the Lithium machinery and brings you
/// back to the sane land of [`Result`], or call [`InFlightException::rethrow`] to piggy-back on the
/// contexts of the caught exception.
// Type invariant: `ex` is a unique pointer to the exception object on the exception stack.
pub struct InFlightException<E> {
    ex: *mut Exception<E>,
}

impl<E> Drop for InFlightException<E> {
    /// Drop the exception, stopping Lithium unwinding.
    #[inline]
    fn drop(&mut self) {
        unsafe { pop(self.ex) }
    }
}

impl<E> InFlightException<E> {
    pub(crate) unsafe fn new(ex: *mut Exception<E>) -> Self {
        Self { ex }
    }

    /// Throw a new exception by reusing the existing context.
    ///
    /// See [`intercept`] docs for examples and safety notes.
    #[inline]
    pub fn rethrow<F>(self, new_cause: F) -> ! {
        let ex = unsafe { replace_last(self.ex, new_cause) };
        core::mem::forget(self);
        unsafe {
            ActiveBackend::throw(ex.cast());
        }
    }
}

/// Begin exception catching.
///
/// If `func` returns a value, this function wraps it in [`Ok`].
///
/// If `func` throws an exception, the error cause along with a handle to the exception is returned
/// in [`Err`]. This handle can be used to rethrow the exception, possibly modifying its value or
/// type in the process.
///
/// If you always need to catch the exception, use [`try`](super::try()) instead. This function is
/// mostly useful as an analogue of [`Result::map_err`].
///
/// Rust panics are propagated as-is and not caught.
///
/// # Safety
///
/// `func` must only throw exceptions of type `E`. See the safety section of [this module](super)
/// for more information.
///
/// **In addition**, certain requirements are imposed on how the returned [`InFlightException`] is
/// used. In particular, no exceptions may be thrown between the moment this function returns
/// an [`InFlightException`] and the moment it is dropped (either by calling [`drop`] or by calling
/// its [`InFlightException::rethrow`] method).
///
/// Caught exceptions are not subject to this requirement, i.e. the following pattern is safe:
///
/// ```rust
/// use lithium::*;
///
/// unsafe {
///     let result = intercept::<(), i32>(|| throw::<i32>(1));
///     drop(intercept::<(), i32>(|| throw::<i32>(2)));
///     drop(result);
/// }
/// ```
///
/// # Example
///
/// ```rust
/// use anyhow::{anyhow, Error, Context};
/// use lithium::*;
///
/// /// Throws [`Error`].
/// unsafe fn f() {
///     throw::<Error>(anyhow!("f failed"));
/// }
///
/// /// Throws [`Error`].
/// unsafe fn g() {
///     // SAFETY:
///     // - f only ever throws Error
///     // - no exception is thrown between `intercept` returning and call to `rethrow`
///     match intercept::<_, Error>(|| f()) {
///         Ok(x) => x,
///         Err((e, handle)) => handle.rethrow(e.context("in g")),
///     }
/// }
///
/// // SAFETY: g only ever throws Error
/// println!("{}", unsafe { r#try::<_, Error>(|| g()) }.unwrap_err());
/// ```
#[allow(clippy::missing_errors_doc)]
#[inline]
pub unsafe fn intercept<R, E>(func: impl FnOnce() -> R) -> Result<R, (E, InFlightException<E>)> {
    unsafe { ActiveBackend::intercept(func) }.map_err(|ex| {
        let ex: *mut Exception<E> = ex.cast();
        let ex_ref = unsafe { &*ex };
        let cause = unsafe { ex_ref.cause() };
        (cause, InFlightException { ex })
    })
}
