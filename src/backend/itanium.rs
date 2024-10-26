use super::Backend;
use core::mem::{ManuallyDrop, MaybeUninit};

pub const LITHIUM_EXCEPTION_CLASS: u64 = u64::from_ne_bytes(*b"RUSTLITH");

pub struct ActiveBackend;

unsafe impl Backend for ActiveBackend {
    type ExceptionHeader = Header;

    fn new_header() -> Header {
        Header {
            class: LITHIUM_EXCEPTION_CLASS,
            cleanup: Some(cleanup),
            private: MaybeUninit::uninit(),
        }
    }

    unsafe fn throw(ex: *mut Header) -> ! {
        #[expect(clippy::used_underscore_items)]
        unsafe {
            _Unwind_RaiseException(ex);
        }
    }

    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut Header> {
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

        // Take care not to create a reference to the whole header, as it may theoretically alias
        // for foreign exceptions
        if unsafe { (*ex).class } != LITHIUM_EXCEPTION_CLASS {
            #[expect(clippy::used_underscore_items)]
            unsafe {
                _Unwind_RaiseException(ex);
            }
        }

        Err(ex)
    }
}

#[repr(C)]
pub struct Header {
    class: u64,
    cleanup: Option<unsafe extern "C" fn(i32, *mut Header)>,
    private: MaybeUninit<[*const (); 2]>,
}

unsafe extern "C" fn cleanup(_code: i32, _ex: *mut Header) {
    eprintln!(
        "A Lithium exception was caught by a non-Lithium catch mechanism. This is undefined behavior. The process will now terminate.",
    );
    std::process::abort();
}

extern "C-unwind" {
    fn _Unwind_RaiseException(ex: *mut Header) -> !;
}
