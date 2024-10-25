use super::exception::Exception;
use core::mem::{ManuallyDrop, MaybeUninit};

extern "C-unwind" {
    fn _Unwind_RaiseException(ex: *mut Header) -> !;
}

pub const LITHIUM_EXCEPTION_CLASS: u64 = u64::from_ne_bytes(*b"RUSTLITH");

#[repr(C)]
pub struct Header {
    class: u64,
    cleanup: Option<unsafe extern "C" fn(i32, *mut Header)>,
    private: MaybeUninit<[*const (); 2]>,
}

pub type Align = Header;

impl Header {
    pub fn new() -> Self {
        Self {
            class: LITHIUM_EXCEPTION_CLASS,
            cleanup: Some(cleanup),
            private: MaybeUninit::uninit(),
        }
    }
}

unsafe extern "C" fn cleanup(_code: i32, _ex: *mut Header) {
    eprintln!(
        "A Lithium exception was caught by a non-Lithium catch mechanism. The process will now terminate.",
    );
    std::process::abort();
}

pub unsafe fn throw<E>(_is_local: bool, ex: *mut Exception<E>) -> ! {
    #[expect(clippy::used_underscore_items)]
    unsafe {
        _Unwind_RaiseException(ex.cast());
    }
}

pub unsafe fn intercept<Func: FnOnce() -> R, R, E>(func: Func) -> Result<R, *mut Exception<E>> {
    union Data<Func, R> {
        func: ManuallyDrop<Func>,
        result: ManuallyDrop<R>,
        ex: *mut Header,
    }

    #[inline]
    fn do_call<Func: FnOnce() -> R, R>(data: *mut u8) {
        let data: &mut Data<Func, R> = unsafe { &mut *data.cast() };
        let func = unsafe { ManuallyDrop::take(&mut data.func) };
        data.result = ManuallyDrop::new(func());
    }

    #[inline]
    fn do_catch<Func: FnOnce() -> R, R>(data: *mut u8, ex: *mut u8) {
        let data: &mut Data<Func, R> = unsafe { &mut *data.cast() };
        data.ex = ex.cast();
    }

    let mut data = Data {
        func: ManuallyDrop::new(func),
    };

    if unsafe {
        core::intrinsics::catch_unwind(
            do_call::<Func, R>,
            (&raw mut data).cast(),
            do_catch::<Func, R>,
        )
    } == 0
    {
        return Ok(ManuallyDrop::into_inner(unsafe { data.result }));
    }

    let ex = unsafe { data.ex };

    // Take care not to create a reference to the whole header, as it may theoretically alias for
    // foreign exceptions
    if unsafe { (*ex).class } != LITHIUM_EXCEPTION_CLASS {
        #[expect(clippy::used_underscore_items)]
        unsafe {
            _Unwind_RaiseException(ex);
        }
    }

    Err(ex.cast())
}
