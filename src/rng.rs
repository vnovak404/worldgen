/// Deterministic RNG based on splitmix64/32. No stateful RNG in inner loops.

#[inline]
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[inline]
pub fn splitmix32(mut x: u32) -> u32 {
    x = x.wrapping_add(0x9E3779B9);
    let mut z = x;
    z = (z ^ (z >> 16)).wrapping_mul(0x7FEB352D);
    z = (z ^ (z >> 15)).wrapping_mul(0x846CA68B);
    z ^ (z >> 16)
}

#[inline]
pub fn seed_u32(seed: u64, salt: u64) -> u32 {
    splitmix64(seed ^ salt) as u32
}

#[inline]
pub fn hash2(ix: i32, iy: i32, seed: u32) -> u32 {
    let x = ix as u32;
    let y = iy as u32;
    let mut h = seed ^ 0x9E3779B9;
    h = splitmix32(h ^ x.wrapping_mul(0x85EBCA6B));
    h = splitmix32(h ^ y.wrapping_mul(0xC2B2AE35));
    h
}

/// Simple sequential RNG for plate generation (not used in pixel inner loops).
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = splitmix64(self.state);
        self.state
    }

    pub fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16777216.0
    }

    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }

    pub fn range_usize(&mut self, max: usize) -> usize {
        (self.next_u64() % max as u64) as usize
    }
}
