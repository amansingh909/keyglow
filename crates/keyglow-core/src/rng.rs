/// Small seeded generator (splitmix64). Not for anything security related.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in 0..1 (never 1.0).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in 0..n; returns 0 when n is 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }
    #[test]
    fn floats_and_below_stay_in_range() {
        let mut r = Rng::new(0);
        for _ in 0..1000 {
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
            assert!(r.below(5) < 5);
        }
        assert_eq!(r.below(0), 0);
    }
}
