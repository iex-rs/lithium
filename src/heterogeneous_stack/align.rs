/// Returns the size of `T`, rounded up to the alignment of `AlignAs`.
///
/// This also statically asserts that `T` is valid to store in a container aligned to `AlignAs`,
/// i.e. `T` is no more aligned than `AlignAs`.
pub const fn get_rounded_size<AlignAs, T>() -> usize {
    const {
        assert!(align_of::<T>() <= align_of::<AlignAs>(), "T is overaligned");
    }
    // This cannot panic, as size_of::<T>() < (1 << (usize::BITS - 1)), and thus rounding to
    // a multiple of alignment, which is a power of two, never overflows.
    size_of::<T>().next_multiple_of(align_of::<AlignAs>())
}

pub fn assert_aligned<AlignAs>(n: usize) {
    // Clippy thinks % can panic here, but the divisor is never 0 and we're working in unsigned
    #[expect(clippy::arithmetic_side_effects)]
    let modulo = n % align_of::<AlignAs>();
    assert!(modulo == 0, "Unaligned");
}
