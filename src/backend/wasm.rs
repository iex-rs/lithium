use super::{super::intrinsic::intercept, ThrowByPointer};

pub(crate) struct ActiveBackend;

// SAFETY: We use Wasm EH, which supports nested exceptions correctly. Wasm VM unwinds exceptions
// without touching the exception data, so we don't have to provide a header. As we require that our
// exceptions don't pass through foreign frames, we could throw garbage if we wanted -- we just need
// to be able to differentiate between Rust or foreign panics and Lithium exceptions. We do that by
// assuming that "normal" exceptions are aligned, and an odd address can be used as a marker.
unsafe impl ThrowByPointer for ActiveBackend {
    type ExceptionHeader = Header;

    fn new_header() -> Header {
        Header
    }

    #[inline]
    unsafe fn throw(ex: *mut Header) -> ! {
        // SAFETY: Wasm has no unwinder, so the pointer reaches `intercept` as-is.
        unsafe {
            core::arch::wasm32::throw::<0>(ex.cast::<u8>().wrapping_add(1));
        }
    }

    #[inline(always)]
    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut Header> {
        let ex = match intercept(func, |ex| ex) {
            Ok(value) => return Ok(value),
            Err(ex) => ex,
        };

        if ex.addr() & 1 == 0 {
            // SAFETY: We're rethrowing a foreign exception we've just caught, this is necessarily
            // safe because it's indistinguishable from not catching it in the first place due to
            // Wasm EH being performed by the VM.
            unsafe {
                core::arch::wasm32::throw::<0>(ex);
            }
        }

        // Any other language or runtime unwinding through foreign code (which Lithium is, to them)
        // will have to use Itanium EH ABI, which requires `ex` to be aligned. So if `ex` is
        // unaligned, it's necessarily our exception.
        Err(ex.wrapping_sub(1).cast())
    }
}

#[repr(C, align(2))]
pub struct Header;
