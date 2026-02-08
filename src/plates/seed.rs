use crate::rng::Rng;

/// Poisson disk sampling for plate centers.
/// Attempts to place `count` points with minimum separation.
/// Relaxes distance constraint if stuck.
pub fn poisson_plate_seeds(w: usize, h: usize, count: usize, seed: u64) -> Vec<[f32; 2]> {
    let mut rng = Rng::new(seed ^ 0xA1B2C3D4E5F60789);
    let mut min_dist = ((w * h) as f32 / count as f32).sqrt() * 0.6;
    let mut seeds: Vec<[f32; 2]> = Vec::with_capacity(count);
    let mut attempts = 0usize;
    let relax_interval = count * 200;

    while seeds.len() < count && attempts < count * 2000 {
        let x = rng.range_f32(0.0, w as f32);
        let y = rng.range_f32(0.0, h as f32);

        let ok = seeds.iter().all(|s| {
            let dx = s[0] - x;
            let dy = s[1] - y;
            (dx * dx + dy * dy).sqrt() >= min_dist
        });

        if ok {
            seeds.push([x, y]);
        }
        attempts += 1;
        if attempts % relax_interval == 0 {
            min_dist *= 0.85;
        }
    }

    // Fallback: fill remaining randomly
    while seeds.len() < count {
        seeds.push([rng.range_f32(0.0, w as f32), rng.range_f32(0.0, h as f32)]);
    }

    seeds
}
