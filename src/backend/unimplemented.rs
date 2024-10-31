use super::{RethrowHandle, ThrowByValue};

pub(crate) struct ActiveBackend;

compile_error!("Lithium does not support no_std in this configuration");

unsafe impl ThrowByValue for ActiveBackend {
    type RethrowHandle<E> = UnimplementedRethrowHandle;

    unsafe fn throw<E>(_cause: E) -> ! {
        unimplemented!()
    }

    unsafe fn intercept<Func: FnOnce() -> R, R, E>(
        _func: Func,
    ) -> Result<R, (E, Self::RethrowHandle<E>)> {
        unimplemented!()
    }
}

pub(crate) struct UnimplementedRethrowHandle;

impl RethrowHandle for UnimplementedRethrowHandle {
    unsafe fn rethrow<F>(self, _new_cause: F) -> ! {
        unimplemented!()
    }
}
