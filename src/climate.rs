use rayon::prelude::*;

use crate::config::Params;
use crate::grid::Grid;
use crate::noise::fbm;
use crate::rng::seed_u32;

const SALT_TEMP: u64 = 0xC11_CAFE_0001;
const SALT_PRECIP: u64 = 0xC11_CAFE_0002;

/// Smoothstep: 0 at edge0, 1 at edge1.
#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Compute temperature grid (Celsius) from elevation.
/// - Latitude gradient: 30C at equator → -30C at poles (lat^1.5 curve)
/// - Lapse rate: -6.5C per 1000m for land above sea level
/// - Small FBM noise for local variation
pub fn compute_temperature(height: &Grid<f32>, seed: u64) -> Grid<f32> {
    let w = height.w;
    let h = height.h;
    let mut temp = Grid::new(w, h);
    let noise_seed = seed_u32(seed, SALT_TEMP);

    temp.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let lat = ((y as f32 / h as f32) - 0.5).abs() * 2.0; // 0 at equator, 1 at poles
        let base_temp = 30.0 - 60.0 * lat.powf(1.5);
        for x in 0..w {
            let elev = height.get(x, y);
            let mut t = base_temp;
            // Lapse rate for land above sea level
            if elev > 0.0 {
                t -= 6.5 * elev / 1000.0;
            }
            // Small FBM noise ±2C
            let nx = x as f32 / w as f32 * 8.0;
            let ny = y as f32 / h as f32 * 8.0;
            t += fbm(nx, ny, noise_seed, 4, 1.0, 2.0, 0.5) * 2.0;
            row[x] = t;
        }
    });

    temp
}

/// Compute precipitation grid (mm/year) using Hadley-cell wind model + moisture advection.
pub fn compute_precipitation(
    height: &Grid<f32>,
    temperature: &Grid<f32>,
    seed: u64,
    params: &Params,
) -> Grid<f32> {
    let w = height.w;
    let h = height.h;
    let mut precip = Grid::new(w, h);
    let _noise_seed = seed_u32(seed, SALT_PRECIP);

    // Row-wise moisture advection along prevailing winds
    precip.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let lat_frac = (y as f32 / h as f32 - 0.5).abs() * 2.0; // 0..1
        let lat_deg = lat_frac * 90.0;

        // Wind direction from Hadley cells with smooth transitions
        // Trade winds (0-30°): easterly dx=-1
        // Westerlies (30-60°): dx=+1
        // Polar easterlies (60-90°): dx=-1
        let dx: f32 = {
            let trade_to_west = smoothstep(25.0, 35.0, lat_deg);
            let west_to_polar = smoothstep(55.0, 65.0, lat_deg);
            let trade = -1.0;
            let westerly = 1.0;
            let polar = -1.0;
            let tw = trade * (1.0 - trade_to_west) + westerly * trade_to_west;
            tw * (1.0 - west_to_polar) + polar * west_to_polar
        };

        let warmup = w / 4;
        let total_steps = warmup + w;

        // Moisture capacity: gentler scaling than real Clausius-Clapeyron.
        // Real C-C doubles per 10°C → 40:1 equator-to-pole ratio (too extreme for visuals).
        // Use doubling per 20°C → ~6:1 ratio, plus a floor so polar air still carries moisture.
        let capacity_for_temp = |temp_c: f32| -> f32 {
            let base_cap = 50.0;
            let cc = base_cap * (2.0_f32).powf(temp_c / 20.0);
            cc.clamp(15.0, 200.0) // floor at 15 so polar regions still get rain
        };

        let mut moisture: f32 = 0.0;
        let mut recorded = vec![0.0f32; w];

        let start_x: i32 = if dx > 0.0 { -(warmup as i32) } else { w as i32 - 1 + warmup as i32 };
        let step: i32 = if dx > 0.0 { 1 } else { -1 };

        for s in 0..total_steps {
            let raw_x = start_x + step * s as i32;
            let x = ((raw_x % w as i32) + w as i32) as usize % w;

            let elev = height.get(x, y);
            let temp_c = temperature.get(x, y);
            let cap = capacity_for_temp(temp_c);
            let is_ocean = elev <= 0.0;

            if is_ocean {
                // Over ocean: moisture recharges toward capacity
                let recharge_rate = 0.05;
                moisture += (cap - moisture) * recharge_rate;
            } else {
                // Over land: precipitation depletes moisture
                let base_depletion = 0.025;

                // Orographic lift: extra depletion for upslopes
                let prev_x = ((raw_x - step) % w as i32 + w as i32) as usize % w;
                let elev_prev = height.get(prev_x, y);
                let slope = (elev - elev_prev).max(0.0);
                let orographic = 0.0005 * slope;

                let depletion = (base_depletion + orographic).min(0.5);
                let rain = moisture * depletion;
                moisture -= rain;

                // Evapotranspiration: vegetation and soil recycle moisture back
                // into the atmosphere. Warmer = more evaporation (0.1 at -10C, 0.5 at 30C).
                // This is what keeps continental interiors (Amazon, Congo) wet.
                let evap_frac = 0.1 + 0.4 * smoothstep(-10.0, 30.0, temp_c);
                moisture += rain * evap_frac;

                // Small convective contribution: solar heating drives local
                // updrafts that generate rainfall from any available moisture,
                // even deep inside continents. Scales with temperature.
                let convective = 0.3 * smoothstep(5.0, 30.0, temp_c);
                moisture += convective;

                if s >= warmup {
                    recorded[x] += rain;
                }
            }

            moisture = moisture.clamp(0.0, cap * 1.5);
        }

        for x in 0..w {
            row[x] = recorded[x];
        }
    });

    // Latitude modulation: ITCZ boost + subtropical suppression + mid-latitude cyclonic
    for y in 0..h {
        let lat_frac = (y as f32 / h as f32 - 0.5).abs() * 2.0;
        let lat_deg = lat_frac * 90.0;

        // ITCZ: modest boost at equator (±8°)
        let itcz = 1.0 + 0.3 * (-lat_deg * lat_deg / (2.0 * 8.0 * 8.0)).exp();

        // Subtropical suppression: mild dip at ~28° (desert belts)
        let sub_dist = lat_deg - 28.0;
        let subtropical = 1.0 - 0.3 * (-sub_dist * sub_dist / (2.0 * 8.0 * 8.0)).exp();

        // Mid-latitude cyclonic boost: frontal systems deliver extra moisture 40-60°
        let mid_dist = lat_deg - 50.0;
        let midlat = 1.0 + 0.4 * (-mid_dist * mid_dist / (2.0 * 12.0 * 12.0)).exp();

        for x in 0..w {
            let i = y * w + x;
            precip.data[i] *= itcz * subtropical * midlat;
        }
    }

    // Light N-S blur (sigma ~4 rows) to smooth latitude-band artifacts
    let sigma: f32 = 4.0;
    let radius = (sigma * 3.0).ceil() as i32;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|d| (-((d as f32).powi(2)) / (2.0 * sigma * sigma)).exp())
        .collect();
    let ksum: f32 = kernel.iter().sum();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / ksum).collect();

    let mut blurred = Grid::new(w, h);
    // Blur in y direction (column-wise), parallelized by column
    blurred.data.par_chunks_mut(1).enumerate().for_each(|(i, out)| {
        let x = i % w;
        let y = i / w;
        let mut sum = 0.0f32;
        for (ki, dy) in (-radius..=radius).enumerate() {
            let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
            sum += precip.get(x, sy) * kernel[ki];
        }
        out[0] = sum;
    });

    // Scale to mm/year. The raw values are arbitrary moisture units.
    // Normalize so global land mean ≈ 800mm, then apply rainfall_scale.
    let mut land_sum = 0.0f64;
    let mut land_count = 0u64;
    for i in 0..w * h {
        if height.data[i] > 0.0 {
            land_sum += blurred.data[i] as f64;
            land_count += 1;
        }
    }
    let land_mean = if land_count > 0 { land_sum / land_count as f64 } else { 1.0 };
    let scale = if land_mean > 1e-10 { 800.0 / land_mean } else { 1.0 };
    let scale = scale as f32 * params.rainfall_scale;

    for v in blurred.data.iter_mut() {
        *v = (*v * scale).max(0.0);
    }

    blurred
}
