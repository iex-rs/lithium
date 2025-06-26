use super::{RethrowHandle, ThrowByValue};

pub(crate) struct ActiveBackend;

compile_error!("Lithium does not support builds without std on this platform");

unsafe impl ThrowByValue for ActiveBackend {
    type RethrowHandle<E> = UnimplementedRethrowHandle;

    unsafe fn throw<E>(_cause: E) -> ! {
        unimplemented!()
    }

    fn intercept<Func: FnOnce() -> R, R, E>(_func: Func) -> Result<R, (E, Self::RethrowHandle<E>)> {
        unimplemented!()
    }
}

#[derive(Debug)]
pub(crate) struct UnimplementedRethrowHandle;

impl RethrowHandle for UnimplementedRethrowHandle {
    unsafe fn rethrow<F>(self, _new_cause: F) -> ! {
        unimplemented!()
    }
}
