use rayon::prelude::*;

use crate::config::Params;
use crate::grid::Grid;
use crate::noise::{fbm, ridged_fbm};
use crate::plates::boundary::{CONVERGENT, DIVERGENT, TRANSFORM};
use crate::plates::properties::PlateSet;
use crate::rng::seed_u32;

const SALT_DETAIL: u64 = 0xE1E7_DE7A_1100_FACE;
const SALT_RIDGE: u64 = 0x21D6_E500_CAFE_BABE;
const SALT_COAST: u64 = 0xC0A5_7FAD_1E51_1A1D;
const SALT_WARP: u64 = 0xDA12_BEEF_0000_CAFE;
const SALT_INTERIOR: u64 = 0x1A7E_21A1_0001_0001;
const SALT_CHAIN: u64 = 0xC4A1_BEEF_DEAD_0042;
const SALT_BASE: u64 = 0xBA5E_E1EF_DEAD_CAFE;

/// Build the elevation field from plate properties and boundary distance fields.
/// Elevation is driven by geology (plate boundaries), not noise.
/// Noise is used only for texture and coastline irregularity.
///
/// All pixel-based parameters scale with resolution relative to 1024-wide reference,
/// so the same slider values produce the same geographic features at any resolution.
pub fn build_elevation(
    plate_id: &Grid<u16>,
    plates: &PlateSet,
    btype_grid: &Grid<u8>,
    dist_grid: &Grid<f32>,
    near_bx: &Grid<u16>,
    near_by: &Grid<u16>,
    pa_grid: &Grid<u16>,
    pb_grid: &Grid<u16>,
    major_grid: &Grid<u8>,
    seed: u64,
    params: &Params,
) -> Grid<f32> {
    let w = plate_id.w;
    let h = plate_id.h;
    let n = w * h;

    // Resolution scale: all pixel-based params are authored for 2048-wide.
    let scale = w as f32 / 2048.0;

    let detail_seed = seed_u32(seed, SALT_DETAIL);
    let ridge_seed = seed_u32(seed, SALT_RIDGE);
    let coast_seed = seed_u32(seed, SALT_COAST);
    let warp_seed = seed_u32(seed, SALT_WARP);
    let interior_seed = seed_u32(seed, SALT_INTERIOR);
    let chain_seed = seed_u32(seed, SALT_CHAIN);
    let base_seed = seed_u32(seed, SALT_BASE);

    // Scale pixel-based params
    let mw = params.mountain_width * scale;
    let blur_sigma = params.blur_sigma * scale;
    let shelf_width = params.shelf_width * scale;
    let interior_dist = 80.0 * scale;
    let coast_dist_max = 100.0 * scale;
    let ridge_dist_max = 120.0 * scale;

    // Phase 1: Compute boundary profiles per cell (parallel).
    let profiles: Vec<[f32; 2]> = (0..n)
        .into_par_iter()
        .map(|i| {
            let x = i % w;
            let y = i / w;
            let pid = plate_id.get(x, y) as usize;
            let dist = dist_grid.get(x, y);
            let bx = near_bx.get(x, y) as usize;
            let by = near_by.get(x, y) as usize;
            if bx < w && by < h {
                let btype = btype_grid.get(bx, by);
                let pa = pa_grid.get(bx, by) as usize;
                let pb = pb_grid.get(bx, by) as usize;
                let rate = compute_rate(plates, pa, pb);
                let is_major = major_grid.get(bx, by) != 0;
                let (po, ma) = boundary_profile(btype, dist, rate, pid, pa, pb, is_major, plates, params, scale);

                // Chain modulation: break uniform ridges into individual peaks
                if (po.abs() > 50.0 || ma > 10.0) && dist < mw * 3.0 {
                    let dx = bx as f32 - x as f32;
                    let dy = by as f32 - y as f32;
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let tx = -dy / len;
                    let ty = dx / len;
                    let along = (x as f32 * tx + y as f32 * ty) / w as f32;
                    let across = (x as f32 * ty + y as f32 * (-tx)) / w as f32;
                    let chain = ridged_fbm(
                        along * 6.0, across * 18.0,
                        chain_seed, 3, 1.0, 2.0, 0.5,
                    ).clamp(0.0, 1.0);
                    let m = 0.25 + 0.75 * chain;
                    [po * m, ma * m]
                } else {
                    [po, ma]
                }
            } else {
                [0.0, 0.0]
            }
        })
        .collect();

    let mut profile_off: Vec<f32> = profiles.iter().map(|p| p[0]).collect();
    let mut mt_amp: Vec<f32> = profiles.iter().map(|p| p[1]).collect();

    // Phase 2: Smooth profiles to eliminate Voronoi ridge discontinuities.
    blur_grid(&mut profile_off, w, h, blur_sigma);
    blur_grid(&mut mt_amp, w, h, blur_sigma);

    // Phase 3: Final elevation = base + smoothed profile + noise (parallel).
    let coast_amp = params.coast_amp;
    let interior_amp = params.interior_amp;
    let detail_amp = params.detail_amp;

    let mut height = Grid::<f32>::new(w, h);
    height
        .data
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..w {
                let i = y * w + x;
                let pid = plate_id.get(x, y) as usize;
                let dist = dist_grid.get(x, y);
                let is_continental = plates.is_continental[pid];
                let profile_offset = profile_off[i];
                let mountain_amp = mt_amp[i];

                // Normalized coords for noise
                let u = x as f32 / w as f32;
                let v = y as f32 / h as f32;

                // Domain warping
                let warp_x = fbm(u * 2.0, v * 2.0, warp_seed, 3, 2.0, 2.0, 0.5) * 0.06;
                let warp_y =
                    fbm(u * 2.0 + 17.0, v * 2.0 + 31.0, warp_seed, 3, 2.0, 2.0, 0.5) * 0.06;
                let wu = u + warp_x;
                let wv = v + warp_y;

                // Per-pixel base elevation: noise field + coastal taper.
                let base_center = plates.base_elevation[pid];
                let base_noise = fbm(wu, wv, base_seed, 4, 2.5, 2.0, 0.5);
                let base = if is_continental {
                    let taper = smoothstep((dist / shelf_width).min(1.0));
                    (base_center + base_noise * 500.0) * taper
                } else {
                    base_center + base_noise * 200.0
                };

                // Interior terrain variation
                let interior_noise = if is_continental {
                    let interior_weight = smoothstep((dist / interior_dist).min(1.0));
                    let terrain = fbm(wu, wv, interior_seed, 5, 4.0, 2.1, 0.5);
                    terrain * 350.0 * interior_amp * interior_weight
                } else {
                    fbm(wu, wv, interior_seed, 3, 3.0, 2.0, 0.5) * 150.0 * interior_amp
                };

                // Coastline perturbation
                let coast_perturb = if dist < coast_dist_max {
                    let weight = smoothstep(1.0 - (dist / coast_dist_max).min(1.0));
                    let large = fbm(wu, wv, coast_seed, 3, 3.0, 2.0, 0.5) * 800.0;
                    let small = fbm(wu, wv, coast_seed.wrapping_add(100), 4, 15.0, 2.0, 0.5) * 300.0;
                    (large + small) * weight * coast_amp
                } else {
                    0.0
                };

                // Fine detail noise
                let detail = fbm(wu, wv, detail_seed, 4, 10.0, 2.0, 0.5) * detail_amp;

                // Ridge noise near convergent boundaries
                let ridge = if mountain_amp > 0.0 && dist < ridge_dist_max {
                    let rw1 = fbm(
                        wu * 3.0, wv * 3.0,
                        ridge_seed.wrapping_add(50), 3, 2.0, 2.0, 0.5,
                    ) * 0.10;
                    let rw2 = fbm(
                        wu * 3.0 + 7.3, wv * 3.0 + 2.9,
                        ridge_seed.wrapping_add(51), 3, 2.0, 2.0, 0.5,
                    ) * 0.10;
                    let r = ridged_fbm(wu + rw1, wv + rw2, ridge_seed, 4, 6.0, 2.1, 0.45)
                        .clamp(0.0, 1.0);
                    let falloff = smoothstep(1.0 - (dist / ridge_dist_max).min(1.0));
                    r * mountain_amp * falloff
                } else {
                    0.0
                };

                row[x] = base + profile_offset + coast_perturb + interior_noise + detail + ridge;
            }
        });

    // Continental shelf: smooth transition from coast to deep ocean
    add_continental_shelf(&mut height, shelf_width);

    height
}

/// Separable Gaussian blur with E-W wrapping, clamped N-S.
fn blur_grid(data: &mut Vec<f32>, w: usize, h: usize, sigma: f32) {
    let radius = (sigma * 3.0).ceil() as usize;
    if radius == 0 {
        return;
    }

    let kernel: Vec<f32> = (0..=radius)
        .map(|i| (-(i as f32 * i as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = kernel[0] + 2.0 * kernel[1..].iter().sum::<f32>();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / sum).collect();

    // Horizontal pass (E-W wrap)
    let mut tmp = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = data[y * w + x] * kernel[0];
            for r in 1..=radius {
                s += data[y * w + (x + w - r) % w] * kernel[r];
                s += data[y * w + (x + r) % w] * kernel[r];
            }
            tmp[y * w + x] = s;
        }
    }

    // Vertical pass (clamp at edges)
    for y in 0..h {
        for x in 0..w {
            let mut s = tmp[y * w + x] * kernel[0];
            for r in 1..=radius {
                let uy = y.saturating_sub(r);
                let dy = (y + r).min(h - 1);
                s += tmp[uy * w + x] * kernel[r];
                s += tmp[dy * w + x] * kernel[r];
            }
            data[y * w + x] = s;
        }
    }
}

fn compute_rate(plates: &PlateSet, pid_a: usize, pid_b: usize) -> f32 {
    let va = plates.velocity[pid_a];
    let vb = plates.velocity[pid_b];
    let dvx = va[0] - vb[0];
    let dvy = va[1] - vb[1];
    (dvx * dvx + dvy * dvy).sqrt()
}

/// Returns (elevation_offset, mountain_noise_amplitude) based on boundary type.
/// All pixel-based distances are multiplied by `scale` for resolution independence.
fn boundary_profile(
    btype: u8,
    dist: f32,
    rate: f32,
    current_pid: usize,
    pa: usize,
    pb: usize,
    is_major: bool,
    plates: &PlateSet,
    params: &Params,
    scale: f32,
) -> (f32, f32) {
    let rate_factor = rate.min(2.0);
    let ms = params.mountain_scale;
    let ts = params.trench_scale;
    let mw = params.mountain_width * scale;

    let strength = if is_major { 1.0 } else { 0.35 };

    match btype {
        CONVERGENT => {
            let pa_cont = plates.is_continental[pa];
            let pb_cont = plates.is_continental[pb];

            match (pa_cont, pb_cont) {
                (true, true) => {
                    let peak = (3500.0 + rate_factor * 2000.0) * ms * strength;
                    let offset = peak * gaussian(dist, mw);
                    (offset, (400.0 + rate_factor * 200.0) * ms * strength)
                }
                (true, false) | (false, true) => {
                    if plates.is_continental[current_pid] {
                        let peak = (3000.0 + rate_factor * 1800.0) * ms * strength;
                        let sigma = mw * 0.8;
                        let offset_dist = (dist - 30.0 * scale).max(0.0);
                        let offset = peak * gaussian(offset_dist, sigma);
                        (offset, (300.0 + rate_factor * 150.0) * ms * strength)
                    } else {
                        let trench = -2500.0 * rate_factor.min(1.5) * ts * strength;
                        let offset = trench * gaussian(dist, 12.0 * scale);
                        (offset, 0.0)
                    }
                }
                (false, false) => {
                    if dist < 15.0 * scale {
                        let trench = -1800.0 * rate_factor.min(1.5) * ts * strength;
                        (trench * gaussian(dist, 8.0 * scale), 0.0)
                    } else {
                        let arc = 1000.0 * rate_factor.min(1.5) * ms * strength;
                        let offset = arc * gaussian(dist - 35.0 * scale, 18.0 * scale);
                        (offset, 150.0 * ms * strength)
                    }
                }
            }
        }
        DIVERGENT => {
            let both_oceanic = !plates.is_continental[pa] && !plates.is_continental[pb];

            if both_oceanic {
                let ridge_h = params.ridge_height * rate_factor.min(1.5) * strength;
                (ridge_h * gaussian(dist, 35.0 * scale), 0.0)
            } else {
                let rift = -params.rift_depth * rate_factor.min(1.5) * strength;
                (rift * gaussian(dist, 30.0 * scale), 0.0)
            }
        }
        TRANSFORM => (0.0, 0.0),
        _ => (0.0, 0.0),
    }
}

#[inline]
fn gaussian(dist: f32, sigma: f32) -> f32 {
    (-dist * dist / (2.0 * sigma * sigma)).exp()
}

#[inline]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Continental shelf via distance-from-land chamfer.
fn add_continental_shelf(height: &mut Grid<f32>, shelf_width: f32) {
    let w = height.w;
    let h = height.h;

    let land: Vec<bool> = height.data.iter().map(|&h| h > 0.0).collect();

    let mut coast_dist = vec![f32::MAX; w * h];
    for (i, &is_land) in land.iter().enumerate() {
        if is_land {
            coast_dist[i] = 0.0;
        }
    }

    // Forward chamfer with E-W wrapping
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for (dx, dy, cost) in [
                (-1i32, 0, 1.0f32),
                (0, -1, 1.0),
                (-1, -1, 1.414),
                (1, -1, 1.414),
            ] {
                let ny = y as i32 + dy;
                if ny < 0 || ny >= h as i32 {
                    continue;
                }
                let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                let ni = ny as usize * w + nx;
                let c = coast_dist[ni] + cost;
                if c < coast_dist[i] {
                    coast_dist[i] = c;
                }
            }
        }
    }
    // Backward chamfer with E-W wrapping
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            for (dx, dy, cost) in [
                (1i32, 0, 1.0f32),
                (0, 1, 1.0),
                (1, 1, 1.414),
                (-1, 1, 1.414),
            ] {
                let ny = y as i32 + dy;
                if ny < 0 || ny >= h as i32 {
                    continue;
                }
                let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                let ni = ny as usize * w + nx;
                let c = coast_dist[ni] + cost;
                if c < coast_dist[i] {
                    coast_dist[i] = c;
                }
            }
        }
    }

    // Apply shelf: near-coast ocean gets gentle slope
    for i in 0..w * h {
        if !land[i] && coast_dist[i] < shelf_width {
            let t = coast_dist[i] / shelf_width;
            let st = smoothstep(t);
            let shelf_elev = -250.0 * st;
            height.data[i] = height.data[i].max(shelf_elev);
        }
    }
}
