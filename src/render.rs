use rayon::prelude::*;

use crate::grid::Grid;
use crate::plates::boundary::{CONVERGENT, DIVERGENT, TRANSFORM};
use crate::rng::splitmix32;

// Color palette (adapted from mapper, tuned for meter-scale elevation)
const WATER_DEEP: [u8; 4] = [18, 36, 70, 255];
const WATER_MID: [u8; 4] = [32, 55, 92, 255];
const WATER_SHALLOW: [u8; 4] = [38, 78, 120, 255];
const COAST_SHALLOW: [u8; 4] = [52, 100, 145, 255];
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

/// Diagnostic: render plates colored by macroplate, boundaries distinguished.
/// Major boundaries (between macroplates) = bright white.
/// Minor boundaries (within macroplate) = dim gray.
pub fn render_plates(
    plate_id: &Grid<u16>,
    btype: &Grid<u8>,
    major: &Grid<u8>,
    macro_id: &[usize],
    num_macro: usize,
) -> Vec<u8> {
    let w = plate_id.w;
    let h = plate_id.h;

    // Generate a distinct color per macroplate
    let colors: Vec<[u8; 4]> = (0..num_macro)
        .map(|i| {
            let h = splitmix32(i as u32 * 7 + 123);
            [
                (h & 0xFF) as u8 | 60,
                ((h >> 8) & 0xFF) as u8 | 60,
                ((h >> 16) & 0xFF) as u8 | 60,
                255,
            ]
        })
        .collect();

    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let color = if btype.data[i] != 0 {
                if major.data[i] != 0 {
                    [255, 255, 255, 255] // major boundary = bright white
                } else {
                    [140, 140, 140, 255] // minor boundary = dim gray
                }
            } else {
                let pid = plate_id.data[i] as usize;
                if pid < macro_id.len() {
                    let mid = macro_id[pid];
                    if mid < colors.len() {
                        // Slight shade variation per microplate within macroplate
                        let shade = splitmix32(pid as u32 * 13 + 7);
                        let offset = ((shade & 0x1F) as i16 - 16) as i32;
                        [
                            (colors[mid][0] as i32 + offset).clamp(0, 255) as u8,
                            (colors[mid][1] as i32 + offset).clamp(0, 255) as u8,
                            (colors[mid][2] as i32 + offset).clamp(0, 255) as u8,
                            255,
                        ]
                    } else {
                        [128, 128, 128, 255]
                    }
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
/// Major boundaries = bright, minor = dim.
pub fn render_boundaries(btype: &Grid<u8>, major: &Grid<u8>) -> Vec<u8> {
    let w = btype.w;
    let h = btype.h;
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let is_major = major.data[i] != 0;
        let color = match btype.data[i] {
            CONVERGENT => if is_major { [220, 50, 50, 255] } else { [120, 40, 40, 255] },
            DIVERGENT => if is_major { [50, 80, 220, 255] } else { [40, 50, 120, 255] },
            TRANSFORM => if is_major { [50, 200, 80, 255] } else { [40, 100, 50, 255] },
            _ => [20, 20, 20, 255],
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

// Temperature color stops
const TEMP_COLD: [u8; 4] = [220, 230, 255, 255]; // -30C: white-blue
const TEMP_FREEZE: [u8; 4] = [80, 180, 220, 255]; // 0C: cyan
const TEMP_COOL: [u8; 4] = [60, 160, 80, 255]; // 15C: green
const TEMP_WARM: [u8; 4] = [220, 200, 60, 255]; // 25C: yellow
const TEMP_HOT: [u8; 4] = [200, 50, 30, 255]; // 35C+: red

/// Render temperature map (Celsius).
pub fn render_temperature(temp: &Grid<f32>) -> Vec<u8> {
    let w = temp.w;
    let h = temp.h;
    let mut rgba = vec![0u8; w * h * 4];

    rgba.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let t = temp.get(x, y);
            let color = if t < -30.0 {
                TEMP_COLD
            } else if t < 0.0 {
                lerp_color(TEMP_COLD, TEMP_FREEZE, (t + 30.0) / 30.0)
            } else if t < 15.0 {
                lerp_color(TEMP_FREEZE, TEMP_COOL, t / 15.0)
            } else if t < 25.0 {
                lerp_color(TEMP_COOL, TEMP_WARM, (t - 15.0) / 10.0)
            } else if t < 35.0 {
                lerp_color(TEMP_WARM, TEMP_HOT, (t - 25.0) / 10.0)
            } else {
                TEMP_HOT
            };
            row[x * 4..x * 4 + 4].copy_from_slice(&color);
        }
    });

    rgba
}

// Precipitation color stops
const PRECIP_DRY: [u8; 4] = [200, 180, 130, 255]; // 0mm: tan/desert
const PRECIP_LOW: [u8; 4] = [210, 200, 80, 255]; // 250mm: yellow
const PRECIP_MED: [u8; 4] = [60, 160, 70, 255]; // 1000mm: green
const PRECIP_HIGH: [u8; 4] = [50, 100, 200, 255]; // 2500mm: blue
const PRECIP_VERY_HIGH: [u8; 4] = [20, 40, 120, 255]; // 4000mm+: dark blue

/// Render precipitation map (mm/year).
pub fn render_precipitation(precip: &Grid<f32>) -> Vec<u8> {
    let w = precip.w;
    let h = precip.h;
    let mut rgba = vec![0u8; w * h * 4];

    rgba.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let p = precip.get(x, y);
            let color = if p < 250.0 {
                lerp_color(PRECIP_DRY, PRECIP_LOW, p / 250.0)
            } else if p < 1000.0 {
                lerp_color(PRECIP_LOW, PRECIP_MED, (p - 250.0) / 750.0)
            } else if p < 2500.0 {
                lerp_color(PRECIP_MED, PRECIP_HIGH, (p - 1000.0) / 1500.0)
            } else if p < 4000.0 {
                lerp_color(PRECIP_HIGH, PRECIP_VERY_HIGH, (p - 2500.0) / 1500.0)
            } else {
                PRECIP_VERY_HIGH
            };
            row[x * 4..x * 4 + 4].copy_from_slice(&color);
        }
    });

    rgba
}

// Muted terrain colors for river base map
const RIVER_WATER: [u8; 4] = [30, 45, 65, 255];
const RIVER_LAND_LOW: [u8; 4] = [160, 170, 140, 255];
const RIVER_LAND_HIGH: [u8; 4] = [190, 180, 155, 255];
const RIVER_MTN: [u8; 4] = [210, 205, 195, 255];
const RIVER_BLUE: [u8; 4] = [15, 40, 140, 255];

/// Render rivers overlaid on muted terrain.
pub fn render_rivers(height: &Grid<f32>, river_flow: &Grid<f32>) -> Vec<u8> {
    let w = height.w;
    let h = height.h;
    let mut rgba = vec![0u8; w * h * 4];

    // Find max flow for scaling
    let max_flow = river_flow.data.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    let log_max = max_flow.ln();

    rgba.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let elev = height.get(x, y);
            let flow = river_flow.get(x, y);

            // Light muted terrain base (high contrast against dark blue rivers)
            let base = if elev <= 0.0 {
                RIVER_WATER
            } else {
                let h = elev.min(5000.0);
                if h < 500.0 {
                    lerp_color(RIVER_LAND_LOW, RIVER_LAND_HIGH, h / 500.0)
                } else {
                    lerp_color(RIVER_LAND_HIGH, RIVER_MTN, ((h - 500.0) / 4500.0).min(1.0))
                }
            };

            let color = if flow > 0.0 {
                // Dark blue river, fully opaque — intensity only affects how dark
                let intensity = (flow.ln() / log_max).clamp(0.0, 1.0);
                let alpha = 0.7 + 0.3 * intensity;
                lerp_color(base, RIVER_BLUE, alpha)
            } else {
                base
            };

            row[x * 4..x * 4 + 4].copy_from_slice(&color);
        }
    });

    rgba
}
