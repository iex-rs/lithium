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
/// During unwinding, all destructors of locals must be run, as if `return` was called.
///
/// The user of this trait is allowed to reuse the header when rethrowing exceptions. In particular,
/// the return value of `intercept` may be used as an argument to `throw`.
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
    /// Allocated exceptions, as stored in the [`Exception`](super::exceptions::Exception) type,
    /// will contain this header. This allows exception pointers to be used with ABIs that require
    /// exceptions to contain custom information, like Itanium EH ABI.
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

#[cfg(backend = "seh")]
#[path = "seh.rs"]
mod imp;

#[cfg(all(backend = "panic", feature = "std"))]
#[path = "panic.rs"]
mod imp;

#[cfg(all(backend = "panic", not(feature = "std")))]
#[path = "unimplemented.rs"]
mod imp;

pub(crate) use imp::ActiveBackend;

#[cfg(test)]
mod test {
    use super::*;
    use crate::exceptions::{pop, push, Exception};

    #[test]
    fn intercept_ok() {
        let result = ActiveBackend::intercept(|| String::from("Hello, world!"));
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[test]
    fn intercept_err() {
        let ex = push(String::from("Hello, world!"));
        let result = ActiveBackend::intercept(|| unsafe {
            ActiveBackend::throw(Exception::header(ex));
        });
        let caught_ex = unsafe { Exception::from_header(result.unwrap_err()) };
        assert_eq!(caught_ex, ex);
        assert_eq!(unsafe { (*caught_ex).cause() }, "Hello, world!");
        unsafe {
            pop(caught_ex);
        }
    }

    #[test]
    fn intercept_panic() {
        let result = std::panic::catch_unwind(|| {
            ActiveBackend::intercept(|| std::panic::resume_unwind(Box::new("Hello, world!")))
                .unwrap()
        });
        assert_eq!(
            *result.unwrap_err().downcast_ref::<&'static str>().unwrap(),
            "Hello, world!",
        );
    }

    #[test]
    fn nested_intercept() {
        let ex = push(String::from("Hello, world!"));
        let result = ActiveBackend::intercept(|| {
            ActiveBackend::intercept(|| unsafe {
                ActiveBackend::throw(Exception::header(ex));
            })
        });
        let caught_ex = unsafe { Exception::from_header(result.unwrap().unwrap_err()) };
        assert_eq!(caught_ex, ex);
        assert_eq!(unsafe { (*caught_ex).cause() }, "Hello, world!");
        unsafe {
            pop(caught_ex);
        }
    }

    #[test]
    fn rethrow() {
        let ex1 = push(String::from("Hello, world!"));
        let result = ActiveBackend::intercept(|| {
            let result = ActiveBackend::intercept(|| unsafe {
                ActiveBackend::throw(Exception::header(ex1));
            });
            let ex2 = result.unwrap_err();
            assert_eq!(unsafe { Exception::header(ex1) }, ex2);
            unsafe {
                ActiveBackend::throw(ex2);
            }
        });
        let caught_ex = unsafe { Exception::from_header(result.unwrap_err()) };
        assert_eq!(caught_ex, ex1);
        assert_eq!(unsafe { (*caught_ex).cause() }, "Hello, world!");
        unsafe {
            pop(caught_ex);
        }
    }

    #[test]
    fn destructors_are_run() {
        struct Dropper<'a>(&'a mut bool);
        impl Drop for Dropper<'_> {
            fn drop(&mut self) {
                *self.0 = true;
            }
        }

        let mut destructor_was_run = false;
        let ex1 = push(String::from("Hello, world!"));
        let result = ActiveBackend::intercept(|| {
            let _dropper = Dropper(&mut destructor_was_run);
            unsafe {
                ActiveBackend::throw(Exception::header(ex1));
            }
        });
        let caught_ex1 = unsafe { Exception::from_header(result.unwrap_err()) };
        assert_eq!(caught_ex1, ex1);
        assert_eq!(unsafe { (*caught_ex1).cause() }, "Hello, world!");
        unsafe {
            pop(caught_ex1);
        }

        assert!(destructor_was_run);
    }

    #[test]
    fn nested_with_drop() {
        struct Dropper;
        impl Drop for Dropper {
            fn drop(&mut self) {
                let ex2 = push(String::from("Awful idea"));
                let result = ActiveBackend::intercept(|| unsafe {
                    ActiveBackend::throw(Exception::header(ex2));
                });
                let caught_ex2 = unsafe { Exception::from_header(result.unwrap_err()) };
                assert_eq!(caught_ex2, ex2);
                assert_eq!(unsafe { (*caught_ex2).cause() }, "Awful idea");
                unsafe {
                    pop(caught_ex2);
                }
            }
        }

        let ex1 = push(String::from("Hello, world!"));
        let result = ActiveBackend::intercept(|| {
            let _dropper = Dropper;
            unsafe {
                ActiveBackend::throw(Exception::header(ex1));
            }
        });
        let caught_ex1 = unsafe { Exception::from_header(result.unwrap_err()) };
        assert_eq!(caught_ex1, ex1);
        assert_eq!(unsafe { (*caught_ex1).cause() }, "Hello, world!");
        unsafe {
            pop(caught_ex1);
        }
    }
}
