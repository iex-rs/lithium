use super::Backend;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

pub(crate) struct ActiveBackend;

// SAFETY: We basically use Rust's own mechanism for unwinding (panics), which satisfies all
// requirements.
unsafe impl Backend for ActiveBackend {
    type ExceptionHeader = LithiumMarker;

    fn new_header() -> LithiumMarker {
        LithiumMarker
    }

    unsafe fn throw(ex: *mut LithiumMarker) -> ! {
        // SAFETY: `LithiumMarker` is a ZST, so casting the pointer to a box is safe as long as the
        // pointer is aligned and valid, which it is by the safety requirements of this function.
        let ex = unsafe { Box::from_raw(ex) };
        resume_unwind(ex);
    }

    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut LithiumMarker> {
        catch_unwind(AssertUnwindSafe(func)).map_err(|ex| {
            if ex.is::<LithiumMarker>() {
                // If this is a `LithiumMarker`, it must have been produced by `throw`, because this
                // type is crate-local and we don't use it elsewhere. The safety requirements for
                // `throw` require no messing with unwinding up to `intercept`, so this must have
                // been our exception.
                Box::into_raw(ex).cast()
            } else {
                // If this isn't `LithiumMarker`, it can't be thrown by us, so no exceptions are
                // lost.
                resume_unwind(ex);
            }
        })
    }
}

pub(crate) struct LithiumMarker;
