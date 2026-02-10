use std::collections::BinaryHeap;
use std::cmp::Ordering;

use rayon::prelude::*;

use crate::config::Params;
use crate::grid::Grid;
use crate::noise::fbm;
use crate::rng::seed_u32;

const SALT_MEANDER: u64 = 0xD1A_CAFE_0001;

/// Max cells allowed for hydro grid (256M).
const MAX_HYDRO_CELLS: usize = 256_000_000;

/// Determine upscale factor: target 8x, but auto-reduce if base res is too large.
pub fn hydro_scale(w: usize, h: usize) -> usize {
    let base = w * h;
    for s in (1..=8).rev() {
        if base * s * s <= MAX_HYDRO_CELLS {
            return s;
        }
    }
    1
}

/// Entry for priority flood min-heap (inverted for BinaryHeap max behavior).
#[derive(Clone, Copy)]
struct FloodEntry {
    elev: f32,
    idx: u32,
}

impl PartialEq for FloodEntry {
    fn eq(&self, other: &Self) -> bool { self.idx == other.idx }
}
impl Eq for FloodEntry {}

impl PartialOrd for FloodEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FloodEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: reverse ordering so lowest elevation is popped first
        other.elev.partial_cmp(&self.elev).unwrap_or(Ordering::Equal)
    }
}

/// Bilinear upscale of elevation grid.
fn upscale_bilinear(src: &Grid<f32>, scale: usize) -> Grid<f32> {
    let sw = src.w;
    let sh = src.h;
    let dw = sw * scale;
    let dh = sh * scale;
    let mut dst = Grid::new(dw, dh);

    dst.data.par_chunks_mut(dw).enumerate().for_each(|(dy, row)| {
        let sy_f = (dy as f32 + 0.5) / scale as f32 - 0.5;
        let sy0 = (sy_f.floor() as i32).clamp(0, sh as i32 - 1) as usize;
        let sy1 = (sy0 + 1).min(sh - 1);
        let fy = sy_f - sy0 as f32;

        for dx in 0..dw {
            let sx_f = (dx as f32 + 0.5) / scale as f32 - 0.5;
            let sx0_raw = sx_f.floor() as i32;
            let sx0 = ((sx0_raw % sw as i32) + sw as i32) as usize % sw; // E-W wrap
            let sx1 = (sx0 + 1) % sw; // E-W wrap
            let fx = sx_f - sx0_raw as f32;

            let v00 = src.get(sx0, sy0);
            let v10 = src.get(sx1, sy0);
            let v01 = src.get(sx0, sy1);
            let v11 = src.get(sx1, sy1);

            let top = v00 + (v10 - v00) * fx;
            let bot = v01 + (v11 - v01) * fx;
            row[dx] = top + (bot - top) * fy;
        }
    });

    dst
}

/// Nearest-neighbor upscale for precipitation.
fn upscale_nearest(src: &Grid<f32>, scale: usize) -> Grid<f32> {
    let dw = src.w * scale;
    let dh = src.h * scale;
    let mut dst = Grid::new(dw, dh);

    dst.data.par_chunks_mut(dw).enumerate().for_each(|(dy, row)| {
        let sy = dy / scale;
        for dx in 0..dw {
            let sx = dx / scale;
            row[dx] = src.get(sx, sy);
        }
    });

    dst
}

/// Barnes et al. priority-flood depression filling (in-place).
/// Seeds from ocean cells + top/bottom rows so every land cell drains to the nearest coast.
fn priority_flood(elev: &mut Grid<f32>) {
    let w = elev.w;
    let h = elev.h;
    let n = w * h;
    let mut visited = vec![false; n];
    let mut heap = BinaryHeap::new();

    let offsets: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (-1, 0),           (1, 0),
        (-1, 1),  (0, 1),  (1, 1),
    ];

    // Mark all ocean cells as visited — they are natural outlets, no filling needed.
    for i in 0..n {
        if elev.data[i] <= 0.0 {
            visited[i] = true;
        }
    }

    // Seed from top/bottom rows (polar boundaries) — land cells at poles
    for x in 0..w {
        let idx_top = x;
        if !visited[idx_top] {
            visited[idx_top] = true;
            heap.push(FloodEntry { elev: elev.data[idx_top], idx: idx_top as u32 });
        }
        let idx_bot = (h - 1) * w + x;
        if !visited[idx_bot] {
            visited[idx_bot] = true;
            heap.push(FloodEntry { elev: elev.data[idx_bot], idx: idx_bot as u32 });
        }
    }

    // Seed from coastal ocean cells (those adjacent to unvisited land).
    // This ensures land depressions fill toward the nearest coast, not the poles.
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if elev.data[idx] > 0.0 { continue; } // skip land
            for &(dx, dy) in &offsets {
                let ny = y as i32 + dy;
                if ny < 0 || ny >= h as i32 { continue; }
                let ny = ny as usize;
                let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                let ni = ny * w + nx;
                if !visited[ni] {
                    // This ocean cell borders land — add as seed
                    heap.push(FloodEntry { elev: elev.data[idx], idx: idx as u32 });
                    break;
                }
            }
        }
    }

    while let Some(cell) = heap.pop() {
        let ci = cell.idx as usize;
        let cx = ci % w;
        let cy = ci / w;

        for &(dx, dy) in &offsets {
            let ny = cy as i32 + dy;
            if ny < 0 || ny >= h as i32 { continue; }
            let ny = ny as usize;
            let nx = ((cx as i32 + dx) % w as i32 + w as i32) as usize % w;
            let ni = ny * w + nx;

            if visited[ni] { continue; }
            visited[ni] = true;

            // Fill depression: raise neighbor to at least current cell's elevation.
            // Add tiny epsilon so filled areas slope toward their outlet —
            // without this, D8 can't find a downhill direction on flat filled areas
            // and rivers dead-end inland.
            if elev.data[ni] < cell.elev {
                elev.data[ni] = cell.elev + 1e-5;
            }
            heap.push(FloodEntry { elev: elev.data[ni], idx: ni as u32 });
        }
    }
}

/// Add noise to elevation to create river meanders.
/// Applied BEFORE priority flood so drainage paths curve around noise features
/// while still reaching the coast. Amplitude scales inversely with elevation
/// (more meander on flat plains, less in mountains — matching real physics).
fn add_meander_noise(elev: &mut Grid<f32>, seed: u64) {
    let w = elev.w;
    let noise_seed = seed_u32(seed, SALT_MEANDER);

    elev.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let e = row[x];
            if e > 0.0 {
                // Amplitude fades with elevation: full on plains, weak in mountains.
                // Plains (<200m): 15m noise. Mountains (>2000m): ~2m noise.
                let amp = 15.0 / (1.0 + e / 400.0);

                // Two scales of noise for natural-looking curves:
                // Large sweeps (wavelength ~200 hi-res px ≈ 25 base px ≈ 500km)
                let nx = x as f32 / 200.0;
                let ny = y as f32 / 200.0;
                let large = fbm(nx, ny, noise_seed, 3, 1.0, 2.0, 0.5);

                // Smaller wiggles (wavelength ~60 hi-res px ≈ 8 base px ≈ 150km)
                let nx2 = x as f32 / 60.0;
                let ny2 = y as f32 / 60.0;
                let small = fbm(nx2, ny2, noise_seed ^ 0xFF, 2, 1.0, 2.0, 0.5);

                row[x] += amp * (0.7 * large + 0.3 * small);

                // Clamp: don't let noise push land below sea level, or the
                // priority flood will treat it as ocean and break drainage.
                if row[x] < 0.5 {
                    row[x] = 0.5;
                }
            }
        }
    });
}

/// Compute D8 flow direction for each cell (steepest descent).
/// Returns direction as index 0-7 into the 8-neighbor offset array, or 255 for no-flow (flat/sink).
fn compute_flow_direction(elev: &Grid<f32>) -> Grid<u8> {
    let w = elev.w;
    let h = elev.h;
    let mut flow_dir = Grid::new(w, h);

    let offsets: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (-1, 0),           (1, 0),
        (-1, 1),  (0, 1),  (1, 1),
    ];
    let dist: [f32; 8] = [
        std::f32::consts::SQRT_2, 1.0, std::f32::consts::SQRT_2,
        1.0,                           1.0,
        std::f32::consts::SQRT_2, 1.0, std::f32::consts::SQRT_2,
    ];

    flow_dir.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let e = elev.get(x, y);
            let mut best_dir: u8 = 255;
            let mut best_slope = 0.0f32;

            for (d, &(dx, dy)) in offsets.iter().enumerate() {
                let ny = y as i32 + dy;
                if ny < 0 || ny >= h as i32 { continue; }
                let ny = ny as usize;
                let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                let ne = elev.get(nx, ny);
                let slope = (e - ne) / dist[d];
                if slope > best_slope {
                    best_slope = slope;
                    best_dir = d as u8;
                }
            }

            row[x] = best_dir;
        }
    });

    flow_dir
}

/// Argsort indices by elevation (descending — highest first).
fn argsort_descending(elev: &Grid<f32>) -> Vec<u32> {
    let n = elev.data.len();
    let mut indices: Vec<u32> = (0..n as u32).collect();
    indices.par_sort_unstable_by(|&a, &b| {
        elev.data[b as usize]
            .partial_cmp(&elev.data[a as usize])
            .unwrap_or(Ordering::Equal)
    });
    indices
}

/// Flow accumulation: traverse highest-to-lowest, each cell adds its
/// precipitation + upstream flow to its D8 downstream neighbor.
fn flow_accumulation(
    flow_dir: &Grid<u8>,
    hi_precip: &Grid<f32>,
    sorted: &[u32],
) -> Vec<f32> {
    let w = flow_dir.w;
    let h = flow_dir.h;
    let n = w * h;

    let offsets: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (-1, 0),           (1, 0),
        (-1, 1),  (0, 1),  (1, 1),
    ];

    let downstream_of = |i: usize, dir: u8| -> Option<usize> {
        if dir >= 8 { return None; }
        let x = i % w;
        let y = i / w;
        let (dx, dy) = offsets[dir as usize];
        let ny = y as i32 + dy;
        if ny < 0 || ny >= h as i32 { return None; }
        let ny = ny as usize;
        let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
        Some(ny * w + nx)
    };

    let mut flow = vec![0.0f32; n];
    for i in 0..n {
        flow[i] = hi_precip.data[i];
    }

    for &idx in sorted {
        let i = idx as usize;
        if let Some(ni) = downstream_of(i, flow_dir.data[i]) {
            flow[ni] += flow[i];
        }
    }

    flow
}

/// Downsample flow accumulation: for each base-res cell, take MAX from its scale×scale block.
fn downsample_max(flow: &[f32], hi_w: usize, hi_h: usize, scale: usize) -> Grid<f32> {
    let base_w = hi_w / scale;
    let base_h = hi_h / scale;
    let mut out = Grid::new(base_w, base_h);

    out.data.par_chunks_mut(base_w).enumerate().for_each(|(by, row)| {
        for bx in 0..base_w {
            let mut max_val = 0.0f32;
            for dy in 0..scale {
                let hy = by * scale + dy;
                if hy >= hi_h { continue; }
                for dx in 0..scale {
                    let hx = bx * scale + dx;
                    if hx >= hi_w { continue; }
                    let v = flow[hy * hi_w + hx];
                    if v > max_val { max_val = v; }
                }
            }
            row[bx] = max_val;
        }
    });

    out
}

/// Main hydrology pipeline. Returns base-resolution river_flow grid.
/// Also carves valleys into the provided heightmap along river paths.
pub fn compute_hydrology(
    height: &mut Grid<f32>,
    precipitation: &Grid<f32>,
    _seed: u64,
    params: &Params,
) -> Grid<f32> {
    let w = height.w;
    let h = height.h;
    let scale = hydro_scale(w, h);

    // 1. Upscale elevation (bilinear)
    let mut hi_elev = upscale_bilinear(height, scale);
    let hi_w = hi_elev.w;
    let hi_h = hi_elev.h;

    // 3. Meander noise: small-scale perturbation BEFORE priority flood.
    add_meander_noise(&mut hi_elev, _seed);

    // 4. Priority flood — fill depressions in-place
    priority_flood(&mut hi_elev);

    // 5. D8 flow direction
    let flow_dir = compute_flow_direction(&hi_elev);

    // 6. Argsort by elevation (descending)
    let sorted = argsort_descending(&hi_elev);
    drop(hi_elev);

    // 7. Upscale precipitation (nearest-neighbor)
    let hi_precip = upscale_nearest(precipitation, scale);

    // 8. Flow accumulation
    let flow = flow_accumulation(&flow_dir, &hi_precip, &sorted);
    drop(flow_dir);
    drop(hi_precip);
    drop(sorted);

    // 9. Downsample to base resolution (max in each block)
    let mut river_flow = downsample_max(&flow, hi_w, hi_h, scale);
    drop(flow);

    // Zero out ocean cells
    for i in 0..w * h {
        if height.data[i] <= 0.0 {
            river_flow.data[i] = 0.0;
        }
    }

    // 10. Percentile threshold on raw flow (unchanged from what worked).
    // This preserves river-to-ocean continuity since flow increases monotonically
    // downstream — if a cell passes, every cell downstream of it also passes.
    let mut land_flows: Vec<f32> = river_flow.data.iter().copied().filter(|&v| v > 0.0).collect();
    let flow_threshold = if land_flows.len() > 100 {
        land_flows.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((1.0 - params.river_threshold as f64) * land_flows.len() as f64) as usize;
        let idx = idx.min(land_flows.len() - 1);
        land_flows[idx]
    } else {
        f32::MAX
    };

    // Save raw flow before zeroing (needed for upstream extension)
    let raw_flow: Vec<f32> = river_flow.data.clone();

    for i in 0..w * h {
        if river_flow.data[i] < flow_threshold {
            river_flow.data[i] = 0.0;
        }
    }

    // 11. Per-basin upstream extension: grow rivers into headwaters,
    // but cap additions per river system so dry continents don't flood.
    {
        let offsets: [(i32, i32); 8] = [
            (-1, -1), (0, -1), (1, -1),
            (-1, 0),           (1, 0),
            (-1, 1),  (0, 1),  (1, 1),
        ];

        // Label connected components of the thresholded river network.
        let mut labels = vec![0u32; w * h];
        let mut next_label = 1u32;
        let mut comp_sizes: Vec<u32> = vec![0]; // index 0 unused
        for start in 0..w * h {
            if river_flow.data[start] <= 0.0 || labels[start] != 0 { continue; }
            let label = next_label;
            next_label += 1;
            comp_sizes.push(0);
            let mut stack = vec![start];
            labels[start] = label;
            while let Some(i) = stack.pop() {
                comp_sizes[label as usize] += 1;
                let x = i % w;
                let y = i / w;
                for &(dx, dy) in &offsets {
                    let ny = y as i32 + dy;
                    if ny < 0 || ny >= h as i32 { continue; }
                    let ny = ny as usize;
                    let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                    let ni = ny * w + nx;
                    if river_flow.data[ni] > 0.0 && labels[ni] == 0 {
                        labels[ni] = label;
                        stack.push(ni);
                    }
                }
            }
        }

        // Each component can grow by up to 50% of its original size.
        let mut added = vec![0u32; next_label as usize];
        let max_add: Vec<u32> = comp_sizes.iter()
            .map(|&s| (s as f32 * 0.5).ceil() as u32)
            .collect();

        // Must have meaningful flow to extend (not just noise-level drainage)
        let min_extend_flow = flow_threshold * 0.05;

        for _pass in 0..20 {
            let mut changed = false;
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if river_flow.data[i] > 0.0 { continue; } // already a river
                    if raw_flow[i] < min_extend_flow { continue; } // too little flow

                    // Find which component this cell would join
                    let mut best_label = 0u32;
                    for &(dx, dy) in &offsets {
                        let ny = y as i32 + dy;
                        if ny < 0 || ny >= h as i32 { continue; }
                        let ny = ny as usize;
                        let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                        if labels[ny * w + nx] > 0 {
                            best_label = labels[ny * w + nx];
                            break;
                        }
                    }

                    if best_label == 0 { continue; } // not adjacent to any river
                    if added[best_label as usize] >= max_add[best_label as usize] { continue; } // basin cap reached

                    river_flow.data[i] = raw_flow[i];
                    labels[i] = best_label;
                    added[best_label as usize] += 1;
                    changed = true;
                }
            }
            if !changed { break; }
        }
    }

    // 12. Carve valleys into the heightmap along river paths.
    carve_valleys(height, &river_flow, flow_threshold);

    river_flow
}

/// Carve river valleys into the heightmap.
/// Erosion depth = K * ln(1 + flow/threshold), capped, then blurred to widen valleys.
fn carve_valleys(height: &mut Grid<f32>, river_flow: &Grid<f32>, threshold: f32) {
    let w = height.w;
    let h = height.h;
    let n = w * h;
    let threshold = threshold.max(1.0);

    // Compute raw carving depth per cell
    let mut carve = vec![0.0f32; n];
    for i in 0..n {
        let flow = river_flow.data[i];
        if flow > 0.0 {
            let depth = 25.0 * (1.0 + flow / threshold).ln();
            carve[i] = depth.min(150.0);
        }
    }

    // Widen valleys with separable Gaussian blur (sigma ~1.5 cells)
    let sigma: f32 = 1.5;
    let radius = (sigma * 3.0).ceil() as i32;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|d| (-(d as f32).powi(2) / (2.0 * sigma * sigma)).exp())
        .collect();
    let ksum: f32 = kernel.iter().sum();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / ksum).collect();

    // Blur X (with E-W wrapping)
    let mut temp = vec![0.0f32; n];
    temp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut sum = 0.0f32;
            for (ki, dx) in (-radius..=radius).enumerate() {
                let sx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
                sum += carve[y * w + sx] * kernel[ki];
            }
            row[x] = sum;
        }
    });

    // Blur Y (clamp at poles)
    let mut blurred = vec![0.0f32; n];
    blurred.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut sum = 0.0f32;
            for (ki, dy) in (-radius..=radius).enumerate() {
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                sum += temp[sy * w + x] * kernel[ki];
            }
            row[x] = sum;
        }
    });

    // Apply carving: subtract from heightmap, don't go below sea level
    for i in 0..n {
        if blurred[i] > 0.0 && height.data[i] > 0.0 {
            height.data[i] = (height.data[i] - blurred[i]).max(1.0);
        }
    }
}
