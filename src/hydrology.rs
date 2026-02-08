use std::collections::BinaryHeap;
use std::cmp::Ordering;

use rayon::prelude::*;

use crate::config::Params;
use crate::grid::Grid;

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

            // Fill depression: raise neighbor to at least current cell's elevation
            if elev.data[ni] < cell.elev {
                elev.data[ni] = cell.elev;
            }
            heap.push(FloodEntry { elev: elev.data[ni], idx: ni as u32 });
        }
    }
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

/// Flow accumulation: traverse cells highest-to-lowest.
/// Each cell adds its precipitation + upstream flow to its D8 downstream neighbor.
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

    // Initialize flow with precipitation
    let mut flow = vec![0.0f32; n];
    for i in 0..n {
        flow[i] = hi_precip.data[i];
    }

    // Traverse highest to lowest, push flow downstream
    for &idx in sorted {
        let i = idx as usize;
        let dir = flow_dir.data[i];
        if dir >= 8 { continue; } // no-flow cell

        let x = i % w;
        let y = i / w;
        let (dx, dy) = offsets[dir as usize];
        let ny = y as i32 + dy;
        if ny < 0 || ny >= h as i32 { continue; }
        let ny = ny as usize;
        let nx = ((x as i32 + dx) % w as i32 + w as i32) as usize % w;
        let ni = ny * w + nx;

        flow[ni] += flow[i];
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
pub fn compute_hydrology(
    height: &Grid<f32>,
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

    // 2. Priority flood — fill depressions in-place
    priority_flood(&mut hi_elev);

    // 3. D8 flow direction
    let flow_dir = compute_flow_direction(&hi_elev);

    // 4. Argsort by elevation (descending) — needs hi_elev before drop
    let sorted = argsort_descending(&hi_elev);

    // Drop hi_elev to free memory
    drop(hi_elev);

    // 5. Upscale precipitation (nearest-neighbor)
    let hi_precip = upscale_nearest(precipitation, scale);

    // 6. Flow accumulation
    let flow = flow_accumulation(&flow_dir, &hi_precip, &sorted);

    // Drop intermediates
    drop(flow_dir);
    drop(hi_precip);
    drop(sorted);

    // 7. Downsample to base resolution (max in each block)
    let mut river_flow = downsample_max(&flow, hi_w, hi_h, scale);

    // Drop hi-res flow
    drop(flow);

    // Zero out ocean cells — no rivers on water
    for i in 0..w * h {
        if height.data[i] <= 0.0 {
            river_flow.data[i] = 0.0;
        }
    }

    // Apply threshold: river_threshold is a fraction (0..1) — only the top
    // river_threshold fraction of land cells by flow are shown as rivers.
    // E.g. 0.002 = top 0.2% of land cells.
    let mut land_flows: Vec<f32> = river_flow.data.iter().copied().filter(|&v| v > 0.0).collect();
    let threshold = if land_flows.len() > 100 {
        land_flows.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((1.0 - params.river_threshold as f64) * land_flows.len() as f64) as usize;
        let idx = idx.min(land_flows.len() - 1);
        land_flows[idx]
    } else {
        f32::MAX
    };

    for v in river_flow.data.iter_mut() {
        if *v < threshold {
            *v = 0.0;
        }
    }

    river_flow
}
