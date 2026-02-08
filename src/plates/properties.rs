use crate::grid::Grid;
use crate::rng::Rng;

/// Properties for each plate: type, velocity, base elevation.
pub struct PlateSet {
    pub is_continental: Vec<bool>,
    pub velocity: Vec<[f32; 2]>,
    pub base_elevation: Vec<f32>,
}

pub fn assign_plate_properties(
    num_plates: usize,
    plate_id: &Grid<u16>,
    continental_fraction: f32,
    seed: u64,
) -> PlateSet {
    let mut rng = Rng::new(seed ^ 0xC1A5_51F0_0000_0001);

    // Count cells per plate
    let mut counts = vec![0usize; num_plates];
    for &pid in &plate_id.data {
        if (pid as usize) < num_plates {
            counts[pid as usize] += 1;
        }
    }

    // Classify: largest plates first become continental until fraction met
    let total: usize = counts.iter().sum();
    let mut sorted: Vec<usize> = (0..num_plates).collect();
    sorted.sort_by(|&a, &b| counts[b].cmp(&counts[a]));

    let mut is_continental = vec![false; num_plates];
    let mut remaining = (continental_fraction * total as f32) as usize;
    for &idx in &sorted {
        if remaining == 0 {
            break;
        }
        is_continental[idx] = true;
        remaining = remaining.saturating_sub(counts[idx]);
    }

    // Velocity: random angle + magnitude, enforce net-zero momentum
    let mut velocity = vec![[0.0f32; 2]; num_plates];
    for v in &mut velocity {
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let mag = rng.range_f32(0.3, 1.0);
        *v = [angle.cos() * mag, angle.sin() * mag];
    }
    // Subtract weighted mean to get net-zero momentum
    let (mut sum_vx, mut sum_vy, mut sum_w) = (0.0f32, 0.0f32, 0.0f32);
    for (i, v) in velocity.iter().enumerate() {
        let w = counts[i] as f32;
        sum_vx += v[0] * w;
        sum_vy += v[1] * w;
        sum_w += w;
    }
    if sum_w > 0.0 {
        let bias_x = sum_vx / sum_w;
        let bias_y = sum_vy / sum_w;
        for v in &mut velocity {
            v[0] -= bias_x;
            v[1] -= bias_y;
        }
    }

    // Base elevation
    let mut base_elevation = vec![0.0f32; num_plates];
    for (i, elev) in base_elevation.iter_mut().enumerate() {
        *elev = if is_continental[i] {
            rng.range_f32(200.0, 800.0)
        } else {
            rng.range_f32(-4000.0, -3000.0)
        };
    }

    PlateSet {
        is_continental,
        velocity,
        base_elevation,
    }
}
