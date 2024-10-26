pub fn assert_aligned<AlignAs>(n: usize) {
    // Clippy thinks % can panic here, but the divisor is never 0 and we're working in unsigned
    #[expect(clippy::arithmetic_side_effects)]
    let modulo = n % align_of::<AlignAs>();
    assert!(modulo == 0, "Unaligned");
}
