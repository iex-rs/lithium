use super::{
    backend::{ActiveBackend, Backend},
    exceptions::push,
};

/// Throw an exception.
///
/// If uncaught, exceptions eventually terminate the process or the thread.
///
/// # Safety
///
/// See the safety section of [this module](super) for information on matching types.
///
/// In addition, the caller must ensure that the exception can only be caught by Lithium functions
/// and not by the system runtime. The list of banned functions includes
/// [`std::panic::catch_unwind`] and [`std::thread::spawn`]. This effectively means that all calls
/// to [`throw`] must eventually be wrapped in [`try`](super::try()) or
/// [`intercept`](super::intercept()).
///
/// # Example
///
/// ```should_panic
/// use lithium::*;
///
/// unsafe {
///     throw::<&'static str>("Oops!");
/// }
/// ```
#[inline]
pub unsafe fn throw<E>(cause: E) -> ! {
    let ex = push(cause);
    unsafe {
        ActiveBackend::throw(ex.cast());
    }
}
