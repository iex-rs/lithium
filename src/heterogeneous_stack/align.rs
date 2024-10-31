/// Asserts that `n` is a multiple of `align_of::<AlignAs>()`.
///
/// # Panics
///
/// Panics if `n` is not a multiple of alignment.
pub fn assert_aligned<AlignAs>(n: usize) {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The divisor is never 0 and we're working in unsigned"
    )]
    let modulo = n % align_of::<AlignAs>();
    assert!(modulo == 0, "Unaligned");
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pass() {
        assert_aligned::<u16>(0);
        assert_aligned::<u16>(8);
    }

    #[test]
    #[should_panic]
    fn fail() {
        assert_aligned::<u16>(3);
    }
}
