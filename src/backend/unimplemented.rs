use super::Backend;

pub(crate) struct ActiveBackend;

compile_error!("Lithium does not support no_std in this configuration");

unsafe impl Backend for ActiveBackend {
    type ExceptionHeader = ();

    fn new_header() {}

    unsafe fn throw(_ex: *mut ()) -> ! {
        unimplemented!()
    }

    fn intercept<Func: FnOnce() -> R, R>(_func: Func) -> Result<R, *mut ()> {
        unimplemented!()
    }
}
