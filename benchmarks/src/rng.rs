//! Deterministic pseudo-random number generator.
//!
//! A local `SplitMix64` implementation is used instead of an external crate so
//! that a given seed produces the identical corpus across dependency upgrades,
//! keeping published benchmark numbers reproducible.

/// `SplitMix64` generator (Steele, Lea, and Flood, 2014).
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..bound`. Modulo bias is negligible for the small
    /// bounds used by the corpus generator.
    #[allow(clippy::cast_possible_truncation)]
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound must be positive");
        (self.next_u64() % bound as u64) as usize
    }

    /// Returns a value in `low..=high`.
    pub fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    /// Returns `true` with probability `probability`.
    #[allow(clippy::cast_precision_loss)]
    pub fn chance(&mut self, probability: f64) -> bool {
        // The top 53 bits map exactly onto the f64 mantissa.
        let unit = (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64;
        unit < probability
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// Returns a string of `count` uniformly random ASCII digits.
    pub fn digits(&mut self, count: usize) -> String {
        (0..count)
            .map(|_| char::from(b'0' + u8::try_from(self.below(10)).expect("digit fits in u8")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_is_inclusive_and_bounded() {
        let mut rng = Rng::new(1);
        let values: Vec<usize> = (0..1_000).map(|_| rng.range(3, 5)).collect();
        assert!(values.iter().all(|v| (3..=5).contains(v)));
        assert!(values.contains(&3) && values.contains(&5));
    }
}
