use super::{
    exceptions::{recover_last, Exception},
    InFlightException,
};
use std::any::Any;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

pub struct StackPanicException;

#[repr(C)]
pub struct Header;

impl Header {
    pub fn new() -> Self {
        Self
    }
}

unsafe impl<E> Send for Exception<E> {}

pub unsafe fn throw<E>(is_local: bool, ex: *mut Exception<E>) -> ! {
    let ex = if is_local {
        // StackPanicException is a ZST, so this avoids an allocation
        Box::new(StackPanicException)
    } else {
        let ex = unsafe { Box::from_raw(ex) };
        unsafe { to_any(ex) }
    };
    std::panic::resume_unwind(ex);
}

pub unsafe fn intercept<Func: FnOnce() -> R, R, E>(func: Func) -> Result<R, InFlightException<E>> {
    match catch_unwind(AssertUnwindSafe(func)) {
        Ok(value) => Ok(value),
        Err(ex) => {
            let ex = if ex.is::<StackPanicException>() {
                unsafe { recover_last::<E>() }
            } else if (*ex).type_id() == typeid::of::<Exception<E>>() {
                Box::into_raw(ex).cast()
            } else {
                resume_unwind(ex);
            };
            Err(unsafe { InFlightException::new(ex) })
        }
    }
}

unsafe fn to_any<T: Send>(value: Box<T>) -> Box<dyn Any + Send> {
    unsafe {
        std::mem::transmute::<Box<dyn NonStaticAny + '_>, Box<dyn NonStaticAny + 'static>>(value)
    }
    .to_any()
}

trait NonStaticAny: Send {
    fn to_any(self: Box<Self>) -> Box<dyn Any + Send>
    where
        Self: 'static;
}

impl<T: Send> NonStaticAny for T {
    fn to_any(self: Box<Self>) -> Box<dyn Any + Send>
    where
        Self: 'static,
    {
        self
    }
}
