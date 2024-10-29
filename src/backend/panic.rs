use super::ThrowByPointer;
use core::panic::AssertUnwindSafe;
use std::panic::{catch_unwind, resume_unwind};

pub(crate) struct ActiveBackend;

/// A generic panic-powered backend.
///
/// This backend uses a more generic method than Itanium: stable, cross-platform, and available
/// whenever `panic = "unwind"` is enabled. It is less efficient than throwing exceptions manually,
/// but it's the next best thing.
///
/// What we can't affect performance-wise is the allocation of an `_Unwind_Exception` (at least on
/// Itanium), performed inside `std` with `Box`. What we *do* want to avoid is the allocation of
/// `Box<dyn Any + Send + 'static>`, which stores the panic payload.
///
/// Implementation-wise, the idea is simple. An unsized box stores two words, one used for data and
/// one for RTTI. We can supply a unique value for the RTTI word to be able to recognize our panics,
/// and the data word can just contain the thrown `*mut ExceptionHeader`.
///
/// Luckily, the "Memory layout" section of `Box` docs specifies that only boxes wrapping non-ZST
/// types need to have been allocated by the `Global` allocator, so we aren't even comitting UB as
/// long as we're throwing `Box::<ExceptionHeader>::from_raw(ex)`, as long as `ExceptionHeader` is
/// a ZST.
///
/// The devil, however, is in the details. From the AM point of view,
/// `Box::into_raw(Box::from_raw(ex))` is not a no-op: it enforces uniqueness, which is performed
/// differently under Stacked Borrows and Tree Borrows. Under TB, this is not a problem and our
/// approach is sound.
///
/// Under SB, however, `Box::from_raw` reduces the provenance of the passed pointer to just
/// `ExceptionHeader`, losing information about the surrounding object; thus accessing the original
/// exception after `Box::into_raw` is UB. This is [a well-known deficiency][1] in SB, fixed by TB.
///
/// As far as I am aware, rustc does not use this SB unsoundness for optimizations, so this approach
/// will not cause problems in practical code. So the only question is: what should we do under SB?
/// The obvious approach is to keep the UB, but as Miri stops simulation on UB, this might shadow
/// bugs we're actually interested in; in fact, it might hide bugs in user code when our downstream
/// depenedncies use Miri.
///
/// So instead, we use a very similar approach based on exposed provenance. This is not UB under SB,
/// but it cause deoptimizations elsewhere, so we only enable it conditionally. Pulling a Volkswagen
/// is not something to be proud of, but at least we don't cheat under TB. If this approach turns
/// out to not lead to deoptimizations in practice, we might enable it unconditionally.
///
/// [1]: https://github.com/rust-lang/unsafe-code-guidelines/issues/256
// SAFETY: We basically use Rust's own mechanism for unwinding (panics), which satisfies all
// requirements.
unsafe impl ThrowByPointer for ActiveBackend {
    type ExceptionHeader = LithiumMarker;

    fn new_header() -> LithiumMarker {
        LithiumMarker
    }

    #[inline]
    unsafe fn throw(ex: *mut LithiumMarker) -> ! {
        #[cfg(feature = "sound-under-stacked-borrows")]
        ex.expose_provenance();
        // SAFETY: `LithiumMarker` is a ZST, so casting the pointer to a box is safe as long as the
        // pointer is aligned and valid, which it is by the safety requirements of this function.
        let ex = unsafe { Box::from_raw(ex) };
        resume_unwind(ex);
    }

    #[inline(always)]
    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut LithiumMarker> {
        catch_unwind(AssertUnwindSafe(func)).map_err(|ex| {
            if ex.is::<LithiumMarker>() {
                // If this is a `LithiumMarker`, it must have been produced by `throw`, because this
                // type is crate-local and we don't use it elsewhere. The safety requirements for
                // `throw` require no messing with unwinding up to `intercept`, so this must have
                // been our exception.
                let ex: *mut LithiumMarker = Box::into_raw(ex).cast();
                #[cfg(feature = "sound-under-stacked-borrows")]
                let ex = core::ptr::with_exposed_provenance_mut(ex.addr());
                ex
            } else {
                // If this isn't `LithiumMarker`, it can't be thrown by us, so no exceptions are
                // lost.
                resume_unwind(ex);
            }
        })
    }
}

pub(crate) struct LithiumMarker;
