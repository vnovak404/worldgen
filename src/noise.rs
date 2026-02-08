use crate::rng::hash2;

#[inline]
fn smootherstep(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 2D gradient noise (Perlin-style). Better isotropy than value noise --
/// no grid-aligned diagonal artifacts.
#[inline]
pub fn gradient_noise(x: f32, y: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let sx = smootherstep(fx);
    let sy = smootherstep(fy);

    #[inline]
    fn grad(hash: u32, dx: f32, dy: f32) -> f32 {
        // 16 evenly-spaced unit gradients (every 22.5°).
        // Eliminates the directional bias of 4-gradient Perlin.
        match hash & 15 {
            0  =>  dx,
            1  =>  0.924 * dx + 0.383 * dy,
            2  =>  0.707 * (dx + dy),
            3  =>  0.383 * dx + 0.924 * dy,
            4  =>  dy,
            5  => -0.383 * dx + 0.924 * dy,
            6  =>  0.707 * (-dx + dy),
            7  => -0.924 * dx + 0.383 * dy,
            8  => -dx,
            9  => -0.924 * dx - 0.383 * dy,
            10 =>  0.707 * (-dx - dy),
            11 => -0.383 * dx - 0.924 * dy,
            12 => -dy,
            13 =>  0.383 * dx - 0.924 * dy,
            14 =>  0.707 * (dx - dy),
            _  =>  0.924 * dx - 0.383 * dy,
        }
    }

    let v00 = grad(hash2(ix, iy, seed), fx, fy);
    let v10 = grad(hash2(ix + 1, iy, seed), fx - 1.0, fy);
    let v01 = grad(hash2(ix, iy + 1, seed), fx, fy - 1.0);
    let v11 = grad(hash2(ix + 1, iy + 1, seed), fx - 1.0, fy - 1.0);

    let a = lerp(v00, v10, sx);
    let b = lerp(v01, v11, sx);
    // Scale to approximately [-1, 1] range (raw range is ~[-0.7, 0.7])
    lerp(a, b, sy) * 1.414
}

/// Alias for gradient_noise.
pub fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    gradient_noise(x, y, seed)
}

/// Fractal Brownian Motion with per-octave rotation to break grid alignment.
pub fn fbm(x: f32, y: f32, seed: u32, octaves: u32, freq0: f32, lac: f32, gain: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = freq0;
    let mut norm = 0.0;
    // Rotate ~30° per octave to decorrelate
    const COS30: f32 = 0.866025;
    const SIN30: f32 = 0.5;
    let mut px = x;
    let mut py = y;
    for i in 0..octaves {
        sum += gradient_noise(px * freq, py * freq, seed.wrapping_add(i)) * amp;
        norm += amp;
        amp *= gain;
        freq *= lac;
        let (rx, ry) = (px * COS30 - py * SIN30, px * SIN30 + py * COS30);
        px = rx;
        py = ry;
    }
    if norm > 0.0 { sum / norm } else { 0.0 }
}

/// Ridged FBM with per-octave rotation.
pub fn ridged_fbm(
    x: f32, y: f32, seed: u32, octaves: u32, freq0: f32, lac: f32, gain: f32,
) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = freq0;
    let mut norm = 0.0;
    const COS30: f32 = 0.866025;
    const SIN30: f32 = 0.5;
    let mut px = x;
    let mut py = y;
    for i in 0..octaves {
        let n = gradient_noise(px * freq, py * freq, seed.wrapping_add(i));
        sum += (1.0 - n.abs()) * amp;
        norm += amp;
        amp *= gain;
        freq *= lac;
        let (rx, ry) = (px * COS30 - py * SIN30, px * SIN30 + py * COS30);
        px = rx;
        py = ry;
    }
    if norm > 0.0 { sum / norm } else { 0.0 }
}
