use crate::abort;
use core::mem::ManuallyDrop;

union Data<Func, Catch, T, E> {
    init: (ManuallyDrop<Func>, ManuallyDrop<Catch>),
    ok: ManuallyDrop<T>,
    err: ManuallyDrop<E>,
}

/// Catch unwinding from a function.
///
/// Runs `func`. If `func` doesn't unwind, wraps its return value in `Ok` and returns. If `func`
/// unwinds, runs `catch` inside the catch handler and wraps its return value in `Err`. If `catch`
/// or the destructor of `catch` unwinds, the process aborts.
///
/// The argument to `catch` is target-dependent and matches the exception object as supplied by
/// [`core::intrinsics::catch_unwind`]. See rustc sources for specifics.
#[allow(
    clippy::missing_errors_doc,
    reason = "`Err` value is described immediately"
)]
#[inline]
pub fn intercept<Func: FnOnce() -> T, Catch: FnOnce(*mut u8) -> E, T, E>(
    func: Func,
    catch: Catch,
) -> Result<T, E> {
    let mut data: Data<Func, Catch, T, E> = Data {
        init: (ManuallyDrop::new(func), ManuallyDrop::new(catch)),
    };

    // SAFETY: `do_catch` is marked as `#[rustc_nounwind]`
    if unsafe {
        core::intrinsics::catch_unwind(
            do_call::<Func, Catch, T, E>,
            (&raw mut data).cast(),
            do_catch::<Func, Catch, T, E>,
        )
    } == 0i32
    {
        // SAFETY: If zero was returned, no unwinding happened, so `do_call` must have finished till
        // the assignment to `data.ok`.
        Ok(ManuallyDrop::into_inner(unsafe { data.ok }))
    } else {
        // SAFETY: If a non-zero value was returned, unwinding has happened, so `do_catch` was
        // invoked, thus `data.err` is initialized now.
        Err(ManuallyDrop::into_inner(unsafe { data.err }))
    }
}

// This function should be unsafe, but isn't due to the definition of `catch_unwind`.
#[inline]
fn do_call<Func: FnOnce() -> R, Catch: FnOnce(*mut u8) -> E, R, E>(data: *mut u8) {
    // If `func` succeeds, we need to drop `catch`. If the destructor of `catch` panics, the only
    // possibility is to abort, as we don't want `do_catch` to access a destructed object.
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            abort("internal exception handler attempted to unwind");
        }
    }

    // SAFETY: `data` is provided by the `catch_unwind` intrinsic, which copies the pointer to the
    // `data` variable.
    let data: &mut Data<Func, Catch, R, E> = unsafe { &mut *data.cast() };

    // SAFETY: This function is called at the start of the process, so the `init.0` field is still
    // initialized.
    let func = unsafe { ManuallyDrop::take(&mut data.init.0) };
    let result = func();

    let dropper = Dropper;
    // SAFETY: `init.1` is untouched as of yet.
    unsafe {
        ManuallyDrop::drop(&mut data.init.1);
    }
    let _ = ManuallyDrop::new(dropper);

    data.ok = ManuallyDrop::new(result);
}

// This function should be unsafe, but isn't due to the definition of `catch_unwind`.
#[inline]
#[rustc_nounwind]
fn do_catch<Func: FnOnce() -> R, Catch: FnOnce(*mut u8) -> E, R, E>(data: *mut u8, ex: *mut u8) {
    // SAFETY: `data` is provided by the `catch_unwind` intrinsic, which copies the pointer to the
    // `data` variable.
    let data: &mut Data<Func, Catch, R, E> = unsafe { &mut *data.cast() };
    // SAFETY: This function is called immediately after `do_call` panics, which can only happen at
    // the point when `func` is invoked, so the `init.1` field is still initialized.
    let catch = unsafe { ManuallyDrop::take(&mut data.init.1) };
    data.err = ManuallyDrop::new(catch(ex));
}
