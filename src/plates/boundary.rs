use rayon::prelude::*;

use crate::grid::{Grid, wrap_xy};

use super::properties::PlateSet;

/// Boundary type codes.
pub const INTERIOR: u8 = 0;
pub const CONVERGENT: u8 = 1;
pub const DIVERGENT: u8 = 2;
pub const TRANSFORM: u8 = 3;

/// Extract and classify boundaries.
/// Returns (boundary_type, plate_a, plate_b, is_major).
/// is_major = 1 for boundaries between different macroplates (major tectonic features),
/// is_major = 0 for boundaries within the same macroplate (minor internal features).
pub fn extract_boundaries(
    plate_id: &Grid<u16>,
    plates: &PlateSet,
) -> (Grid<u8>, Grid<u16>, Grid<u16>, Grid<u8>) {
    let w = plate_id.w;
    let h = plate_id.h;
    let mut btype_grid = Grid::<u8>::new(w, h);
    let mut pa_grid = Grid::<u16>::new(w, h);
    let mut pb_grid = Grid::<u16>::new(w, h);
    let mut major_grid = Grid::<u8>::new(w, h);

    // Identify boundary cells and classify (parallel by row)
    let rows: Vec<(usize, Vec<(usize, u8, f32, u16, u16, u8)>)> = (0..h)
        .into_par_iter()
        .map(|y| {
            let mut row_boundaries = Vec::new();
            for x in 0..w {
                let pid = plate_id.get(x, y) as usize;
                let mut best_type = INTERIOR;
                let mut best_rate = 0.0f32;
                let mut best_other = pid as u16;
                let mut best_major: u8 = 0;

                let neighbors: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
                for (dx, dy) in neighbors {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let Some((wnx, wny)) = wrap_xy(nx, ny, w, h) else {
                        continue;
                    };
                    let npid = plate_id.get(wnx, wny) as usize;
                    if npid == pid {
                        continue;
                    }

                    let nl = (dx as f32).hypot(dy as f32);
                    let normal = [dx as f32 / nl, dy as f32 / nl];

                    let va = plates.velocity[pid];
                    let vb = plates.velocity[npid];
                    let vrel = [va[0] - vb[0], va[1] - vb[1]];

                    let dot = vrel[0] * normal[0] + vrel[1] * normal[1];
                    let cross = (vrel[0] * normal[1] - vrel[1] * normal[0]).abs();

                    let (bt, rate) = if dot.abs() > cross {
                        if dot > 0.0 {
                            (CONVERGENT, dot)
                        } else {
                            (DIVERGENT, -dot)
                        }
                    } else {
                        (TRANSFORM, cross)
                    };

                    // Major = different macroplates
                    let is_major = if pid < plates.macro_id.len() && npid < plates.macro_id.len() {
                        if plates.macro_id[pid] != plates.macro_id[npid] { 1u8 } else { 0u8 }
                    } else {
                        0
                    };

                    if rate > best_rate {
                        best_rate = rate;
                        best_type = bt;
                        best_other = npid as u16;
                        best_major = is_major;
                    }
                }

                if best_type != INTERIOR {
                    row_boundaries.push((x, best_type, best_rate, pid as u16, best_other, best_major));
                }
            }
            (y, row_boundaries)
        })
        .collect();

    for (y, row_data) in rows {
        for (x, bt, _rate, pa, pb, major) in row_data {
            btype_grid.set(x, y, bt);
            pa_grid.set(x, y, pa);
            pb_grid.set(x, y, pb);
            major_grid.set(x, y, major);
        }
    }

    (btype_grid, pa_grid, pb_grid, major_grid)
}
