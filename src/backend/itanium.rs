use super::Backend;
use core::mem::{ManuallyDrop, MaybeUninit};

pub const LITHIUM_EXCEPTION_CLASS: u64 = u64::from_ne_bytes(*b"RUSTLITH");

pub(crate) struct ActiveBackend;

// SAFETY: We use Itanium EH ABI, which supports nested exceptions correctly. We can assume we don't
// encounter foreign frames, because that's a safety requirement of `throw`.
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
        // SAFETY: We provide a valid exception header.
        unsafe {
            #[expect(clippy::used_underscore_items)]
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
            // SAFETY: `data` is provided by the `catch_unwind` intrinsic, which copies the pointer
            // to the `data` variable.
            let data: &mut Data<Func, R> = unsafe { &mut *data.cast() };
            // SAFETY: This function is called at the start of the process, so the `func` field is
            // still initialized.
            let func = unsafe { ManuallyDrop::take(&mut data.func) };
            data.result = ManuallyDrop::new(func());
        }

        #[inline]
        fn do_catch<Func: FnOnce() -> R, R>(data: *mut u8, ex: *mut u8) {
            // SAFETY: `data` is provided by the `catch_unwind` intrinsic, which copies the pointer
            // to the `data` variable.
            let data: &mut Data<Func, R> = unsafe { &mut *data.cast() };
            data.ex = ex.cast();
        }

        let mut data = Data {
            func: ManuallyDrop::new(func),
        };

        // SAFETY: `do_catch` doesn't do anything that might unwind
        if unsafe {
            core::intrinsics::catch_unwind(
                do_call::<Func, R>,
                (&raw mut data).cast(),
                do_catch::<Func, R>,
            )
        } == 0i32
        {
            // SAFETY: If zero was returned, no unwinding happened, so `do_call` must have finished
            // till the assignment to `data.result`.
            return Ok(ManuallyDrop::into_inner(unsafe { data.result }));
        }

        // SAFETY: If a non-zero value was returned, unwinding has happened, so `do_catch` was
        // invoked, thus `data.ex` is initialized now.
        let ex = unsafe { data.ex };

        // SAFETY: `ex` is a pointer to an exception object as provided by the unwinder, so it must
        // be valid for reads. It's not explicitly documented that the class is not modified in
        // runtime, but that sounds like common sense.
        if unsafe { (*ex).class } != LITHIUM_EXCEPTION_CLASS {
            // SAFETY: The EH ABI allows rethrowing foreign exceptions under the following
            // conditions:
            // - The exception is not modified or otherwise interacted with. We don't do this,
            //   expect for determining whether it's foreign in the first place.
            // - Runtime EH-related functions are not invoked between catching the exception and
            //   rethrowing it. We don't do that.
            // - The foreign exception is not active at the same time as another exception. We don't
            //   trigger exceptions between catch and rethrow, so we only have to rule out the
            //   foreign exception being nested prior to our catch. This is somewhat complicated:
            //   - If the foreign exception is actually a Rust panic, we know from stdlib's code
            //     that the personality function works just fine with rethrowing regardless of
            //     nesting. This is not a hard proof, but this is highly unlikely to change.
            //   - If the foreign exception was produced neither by Rust, nor by Lithium, the case
            //     is similar to how the behavior of `std::panic::catch_unwind` being unwound by
            //     a foreign exception is undefined; i.e., it's on the user who allows foreign
            //     exceptions to travel through Lithium frames.
            //   If project-ffi-unwind changes the rustc behavior, we might have to update this
            //   code.
            unsafe {
                #[expect(clippy::used_underscore_items)]
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
