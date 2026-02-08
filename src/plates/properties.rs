use crate::grid::Grid;
use crate::noise::fbm;
use crate::rng::{Rng, seed_u32};

pub const SALT_MACRO: u64 = 0xAC20_F1A7_E000_0001;
const SALT_CONTINENT: u64 = 0xC017_1E17_FACE_0001;

/// Properties for the hierarchical plate system.
/// Microplates are the actual grid-level plates (~50).
/// Macroplates are groups of microplates (~8) representing tectonic plates.
pub struct PlateSet {
    pub num_micro: usize,
    pub num_macro: usize,
    pub macro_id: Vec<usize>,       // macro_id[micro_pid] → which macroplate
    pub is_continental: Vec<bool>,   // per microplate (inherited from macroplate)
    pub velocity: Vec<[f32; 2]>,     // per microplate (macro + perturbation)
    pub base_elevation: Vec<f32>,    // per microplate
}

pub fn assign_plate_properties(
    num_micro: usize,
    num_macro: usize,
    micro_seeds: &[[f32; 2]],
    macro_seeds: &[[f32; 2]],
    plate_id: &Grid<u16>,
    continental_fraction: f32,
    boundary_noise: f32,
    seed: u64,
) -> PlateSet {
    let w = plate_id.w;
    let h = plate_id.h;
    let mut rng = Rng::new(seed ^ 0xC1A5_51F0_0000_0001);

    // Assign each microplate to nearest macroplate center (noise-weighted).
    // Per-macroplate noise fields distort the Voronoi tessellation, creating
    // organic macroplate territories instead of geometric circles.
    let macro_noise_seed = seed_u32(seed, 0xBA0B_AB0B_CAFE_0042);
    let mut macro_id = vec![0usize; num_micro];
    for (i, ms) in micro_seeds.iter().enumerate() {
        let u = ms[0] / w as f32;
        let v = ms[1] / h as f32;
        let mut best_d = f32::MAX;
        let mut best_j = 0;
        for (j, mc) in macro_seeds.iter().enumerate() {
            // E-W wrapping distance
            let dx_raw = (ms[0] - mc[0]).abs();
            let dx = dx_raw.min(w as f32 - dx_raw);
            let dy = ms[1] - mc[1];
            let base_d = dx * dx + dy * dy;
            // Unique noise per macroplate for organic grouping
            let n = fbm(u, v, macro_noise_seed.wrapping_add(j as u32), 3, 3.0, 2.0, 0.5);
            let d = base_d * (1.0 + n * boundary_noise).max(0.1);
            if d < best_d {
                best_d = d;
                best_j = j;
            }
        }
        macro_id[i] = best_j;
    }

    // Count cells per microplate and per macroplate
    let mut micro_counts = vec![0usize; num_micro];
    for &pid in &plate_id.data {
        if (pid as usize) < num_micro {
            micro_counts[pid as usize] += 1;
        }
    }

    let mut macro_counts = vec![0usize; num_macro];
    for (i, &count) in micro_counts.iter().enumerate() {
        if i < num_micro {
            macro_counts[macro_id[i]] += count;
        }
    }

    // Continental assignment via noise field sampled at microplate seed positions.
    // This decouples "what's land" from macroplate grouping, producing organic
    // continent shapes with irregular coastlines at microplate resolution.
    // Macroplates still control velocities and major/minor boundary classification.
    let continent_seed = seed_u32(seed, SALT_CONTINENT);
    let mut noise_vals: Vec<(usize, f32)> = (0..num_micro)
        .map(|i| {
            let u = micro_seeds[i][0] / w as f32;
            let v = micro_seeds[i][1] / h as f32;
            // Low-frequency noise creates coherent continent blobs
            let n = fbm(u, v, continent_seed, 3, 2.5, 2.0, 0.5);
            (i, n)
        })
        .collect();

    // Sort by noise value (highest first) and assign continental
    // until we hit the target fraction. This creates continents where
    // the noise field is high, oceans where it's low.
    noise_vals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let total: usize = micro_counts.iter().sum();
    let mut is_continental = vec![false; num_micro];
    let mut remaining = (continental_fraction * total as f32) as usize;
    for &(idx, _) in &noise_vals {
        if remaining == 0 {
            break;
        }
        is_continental[idx] = true;
        remaining = remaining.saturating_sub(micro_counts[idx]);
    }

    // Macroplate velocities: random direction + magnitude, net-zero momentum
    let mut macro_velocity = vec![[0.0f32; 2]; num_macro];
    for v in &mut macro_velocity {
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let mag = rng.range_f32(0.3, 1.0);
        *v = [angle.cos() * mag, angle.sin() * mag];
    }
    // Subtract area-weighted mean for net-zero
    let (mut sx, mut sy, mut sw) = (0.0f32, 0.0f32, 0.0f32);
    for (i, v) in macro_velocity.iter().enumerate() {
        let wt = macro_counts[i] as f32;
        sx += v[0] * wt;
        sy += v[1] * wt;
        sw += wt;
    }
    if sw > 0.0 {
        let bx = sx / sw;
        let by = sy / sw;
        for v in &mut macro_velocity {
            v[0] -= bx;
            v[1] -= by;
        }
    }

    // Microplate velocity = macroplate velocity + small random perturbation
    // This gives minor boundaries a small relative velocity for internal features
    let mut velocity = vec![[0.0f32; 2]; num_micro];
    for i in 0..num_micro {
        let mv = macro_velocity[macro_id[i]];
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let mag = rng.range_f32(0.0, 0.15);
        velocity[i] = [
            mv[0] + angle.cos() * mag,
            mv[1] + angle.sin() * mag,
        ];
    }

    // Base elevation per microplate
    let mut base_elevation = vec![0.0f32; num_micro];
    for (i, elev) in base_elevation.iter_mut().enumerate() {
        *elev = if is_continental[i] {
            rng.range_f32(200.0, 800.0)
        } else {
            rng.range_f32(-4000.0, -3000.0)
        };
    }

    PlateSet {
        num_micro,
        num_macro,
        macro_id,
        is_continental,
        velocity,
        base_elevation,
    }
}
