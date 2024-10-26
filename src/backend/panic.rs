use super::Backend;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

pub struct ActiveBackend;

unsafe impl Backend for ActiveBackend {
    type ExceptionHeader = LithiumMarker;

    fn new_header() -> LithiumMarker {
        LithiumMarker
    }

    unsafe fn throw(ex: *mut LithiumMarker) -> ! {
        let ex = unsafe { Box::from_raw(ex) };
        std::panic::resume_unwind(ex);
    }

    unsafe fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut LithiumMarker> {
        match catch_unwind(AssertUnwindSafe(func)) {
            Ok(value) => Ok(value),
            Err(ex) => {
                if ex.is::<LithiumMarker>() {
                    Err(Box::into_raw(ex).cast())
                } else {
                    resume_unwind(ex);
                }
            }
        }
    }
}

pub struct LithiumMarker;
