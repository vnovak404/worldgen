use rayon::prelude::*;

use crate::grid::Grid;
use crate::plates::boundary::{CONVERGENT, DIVERGENT, TRANSFORM};
use crate::rng::splitmix32;

// Color palette (adapted from mapper, tuned for meter-scale elevation)
const WATER_DEEP: [u8; 4] = [18, 36, 70, 255];
const WATER_MID: [u8; 4] = [38, 64, 102, 255];
const WATER_SHALLOW: [u8; 4] = [56, 110, 150, 255];
const COAST_SHALLOW: [u8; 4] = [92, 140, 170, 255];
const LAND_LOW: [u8; 4] = [70, 130, 62, 255];
const LAND_MID: [u8; 4] = [140, 180, 100, 255];
const LAND_HIGH: [u8; 4] = [190, 170, 120, 255];
const MOUNTAIN_LOW: [u8; 4] = [140, 120, 100, 255];
const MOUNTAIN_HIGH: [u8; 4] = [220, 220, 215, 255];
const SNOW: [u8; 4] = [245, 248, 250, 255];
const BEACH_SAND: [u8; 4] = [210, 200, 160, 255];

#[inline]
fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
        255,
    ]
}

/// Render the final color map.
pub fn render_map(height: &Grid<f32>) -> Vec<u8> {
    let w = height.w;
    let h = height.h;
    let mut rgba = vec![0u8; w * h * 4];

    rgba.par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..w {
                let elev = height.get(x, y);
                let color = if elev <= 0.0 {
                    // Water
                    let depth = (-elev).min(5000.0) / 5000.0;
                    if depth < 0.15 {
                        lerp_color(COAST_SHALLOW, WATER_SHALLOW, depth / 0.15)
                    } else if depth < 0.5 {
                        lerp_color(WATER_SHALLOW, WATER_MID, (depth - 0.15) / 0.35)
                    } else {
                        lerp_color(WATER_MID, WATER_DEEP, (depth - 0.5) / 0.5)
                    }
                } else {
                    // Land
                    let h = elev.min(6000.0);
                    if h < 5.0 {
                        // Beach
                        BEACH_SAND
                    } else if h < 500.0 {
                        let t = (h - 5.0) / 495.0;
                        lerp_color(LAND_LOW, LAND_MID, t)
                    } else if h < 1500.0 {
                        let t = (h - 500.0) / 1000.0;
                        lerp_color(LAND_MID, LAND_HIGH, t)
                    } else if h < 3000.0 {
                        let t = (h - 1500.0) / 1500.0;
                        lerp_color(MOUNTAIN_LOW, MOUNTAIN_HIGH, t)
                    } else {
                        let t = ((h - 3000.0) / 3000.0).min(1.0);
                        lerp_color(MOUNTAIN_HIGH, SNOW, t)
                    }
                };

                let out = &mut row[x * 4..x * 4 + 4];
                out.copy_from_slice(&color);
            }
        });

    rgba
}

/// Diagnostic: render each plate as a random color, boundaries white.
pub fn render_plates(plate_id: &Grid<u16>, btype: &Grid<u8>, num_plates: usize) -> Vec<u8> {
    let w = plate_id.w;
    let h = plate_id.h;

    // Generate a distinct color per plate
    let colors: Vec<[u8; 4]> = (0..num_plates)
        .map(|i| {
            let h = splitmix32(i as u32 * 7 + 123);
            [
                (h & 0xFF) as u8 | 40,
                ((h >> 8) & 0xFF) as u8 | 40,
                ((h >> 16) & 0xFF) as u8 | 40,
                255,
            ]
        })
        .collect();

    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let color = if btype.data[i] != 0 {
                [255, 255, 255, 255] // boundary = white
            } else {
                let pid = plate_id.data[i] as usize;
                if pid < colors.len() {
                    colors[pid]
                } else {
                    [128, 128, 128, 255]
                }
            };
            rgba[i * 4..i * 4 + 4].copy_from_slice(&color);
        }
    }
    rgba
}

/// Diagnostic: boundary types as colors.
pub fn render_boundaries(btype: &Grid<u8>) -> Vec<u8> {
    let w = btype.w;
    let h = btype.h;
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let color = match btype.data[i] {
            CONVERGENT => [220, 50, 50, 255],  // red
            DIVERGENT => [50, 80, 220, 255],    // blue
            TRANSFORM => [50, 200, 80, 255],    // green
            _ => [20, 20, 20, 255],             // dark = interior
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&color);
    }
    rgba
}

/// Diagnostic: grayscale distance field.
pub fn render_distance(dist: &Grid<f32>) -> Vec<u8> {
    let max_d = dist.data.iter().cloned().filter(|d| d.is_finite()).fold(0.0f32, f32::max);
    let max_d = max_d.max(1.0);
    let w = dist.w;
    let h = dist.h;
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let d = dist.data[i].min(max_d);
        let v = ((d / max_d) * 255.0) as u8;
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
    }
    rgba
}

/// Diagnostic: grayscale heightmap.
pub fn render_heightmap(height: &Grid<f32>) -> Vec<u8> {
    let min_h = height.data.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_h = height.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let range = (max_h - min_h).max(1.0);
    let w = height.w;
    let h = height.h;
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let t = (height.data[i] - min_h) / range;
        let v = (t * 255.0).clamp(0.0, 255.0) as u8;
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
    }
    rgba
}
