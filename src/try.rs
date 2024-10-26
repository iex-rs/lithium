use super::intercept;

/// Catch an exception.
///
/// If `func` returns a value, this function wraps it in [`Ok`].
///
/// If `func` throws an exception, this function returns it, wrapped it in [`Err`].
///
/// If you need to rethrow the exception, possibly modifying it in the process, consider using the
/// more efficient [`intercept`](intercept()) function instead of pairing [`try`](try()) with
/// [`throw`](super::throw()).
///
/// Rust panics are propagated as-is and not caught.
///
/// # Safety
///
/// `func` must only throw exceptions of type `E`. See the safety section of [this module](super)
/// for more information.
///
/// # Example
///
/// ```rust
/// use lithium::*;
///
/// // SAFETY: the exception type matches
/// let res = unsafe {
///     r#try::<(), &'static str>(|| throw::<&'static str>("Oops!"))
/// };
///
/// assert_eq!(res, Err("Oops!"));
/// ```
#[allow(clippy::missing_errors_doc)]
#[inline]
pub unsafe fn r#try<R, E>(func: impl FnOnce() -> R) -> Result<R, E> {
    // SAFETY:
    // - `func` only throws `E` by the safety requirement.
    // - `InFlightException` is immediately dropped before returning from `try`, so no exceptions
    //   may be thrown while it's alive.
    unsafe { intercept(func) }.map_err(|(cause, _)| cause)
}
