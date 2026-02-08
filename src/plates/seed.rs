use crate::rng::Rng;

/// Poisson disk sampling for plate centers (uniform density).
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
            let dx_raw = (s[0] - x).abs();
            let dx = dx_raw.min(w as f32 - dx_raw);
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

/// Variable-density Poisson disk sampling for microplate centers.
/// Denser seeding near macroplate boundaries → smaller plates there
/// → cracked-eggshell effect with more detail at major plate edges.
pub fn poisson_variable_seeds(
    w: usize,
    h: usize,
    count: usize,
    seed: u64,
    macro_centers: &[[f32; 2]],
) -> Vec<[f32; 2]> {
    let mut rng = Rng::new(seed ^ 0xA1B2C3D4E5F60789);
    let base_dist = ((w * h) as f32 / count as f32).sqrt() * 0.6;
    let mut seeds: Vec<[f32; 2]> = Vec::with_capacity(count);
    let mut attempts = 0usize;
    let relax_interval = count * 200;
    let mut relax_factor = 1.0f32;

    while seeds.len() < count && attempts < count * 2000 {
        let x = rng.range_f32(0.0, w as f32);
        let y = rng.range_f32(0.0, h as f32);

        // Compute boundary proximity (0 = at macro center, ~1 = on macro boundary)
        let proximity = macro_boundary_proximity(x, y, macro_centers, w);
        // Near boundaries: smaller min_dist → denser packing.
        // min_scale=0.35 means boundary plates ~8x smaller in area than interior plates.
        let min_scale = 0.35;
        let local_dist = base_dist
            * (min_scale + (1.0 - min_scale) * (1.0 - proximity * proximity))
            * relax_factor;

        let ok = seeds.iter().all(|s| {
            let dx_raw = (s[0] - x).abs();
            let dx = dx_raw.min(w as f32 - dx_raw);
            let dy = s[1] - y;
            (dx * dx + dy * dy).sqrt() >= local_dist
        });

        if ok {
            seeds.push([x, y]);
        }
        attempts += 1;
        if attempts % relax_interval == 0 {
            relax_factor *= 0.85;
        }
    }

    // Fallback
    while seeds.len() < count {
        seeds.push([rng.range_f32(0.0, w as f32), rng.range_f32(0.0, h as f32)]);
    }

    seeds
}

/// How close a point is to a macroplate Voronoi boundary.
/// Returns 0 at macroplate centers, approaches 1 at equidistant boundaries.
fn macro_boundary_proximity(x: f32, y: f32, macro_centers: &[[f32; 2]], w: usize) -> f32 {
    let mut d1 = f32::MAX;
    let mut d2 = f32::MAX;
    for mc in macro_centers {
        let dx_raw = (x - mc[0]).abs();
        let dx = dx_raw.min(w as f32 - dx_raw);
        let dy = y - mc[1];
        let d = (dx * dx + dy * dy).sqrt();
        if d < d1 {
            d2 = d1;
            d1 = d;
        } else if d < d2 {
            d2 = d;
        }
    }
    if d2 > 0.0 { (d1 / d2).min(1.0) } else { 0.0 }
}
