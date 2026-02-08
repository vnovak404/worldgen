use rayon::prelude::*;

use crate::grid::{Grid, wrap_xy};

use super::properties::PlateSet;

/// Boundary type codes.
pub const INTERIOR: u8 = 0;
pub const CONVERGENT: u8 = 1;
pub const DIVERGENT: u8 = 2;
pub const TRANSFORM: u8 = 3;

/// Extract and classify boundaries.
/// Returns (boundary_type grid, plate_a grid, plate_b grid).
/// plate_a/plate_b store the two plates on each side of the boundary,
/// allowing stable lookups from the distance field without fragile neighbor searches.
pub fn extract_boundaries(
    plate_id: &Grid<u16>,
    plates: &PlateSet,
) -> (Grid<u8>, Grid<u16>, Grid<u16>) {
    let w = plate_id.w;
    let h = plate_id.h;
    let mut btype_grid = Grid::<u8>::new(w, h);
    let mut pa_grid = Grid::<u16>::new(w, h);
    let mut pb_grid = Grid::<u16>::new(w, h);

    // First pass: identify boundary cells and classify (parallel by row)
    let rows: Vec<(usize, Vec<(usize, u8, f32, u16, u16)>)> = (0..h)
        .into_par_iter()
        .map(|y| {
            let mut row_boundaries = Vec::new();
            for x in 0..w {
                let pid = plate_id.get(x, y) as usize;
                let mut best_type = INTERIOR;
                let mut best_rate = 0.0f32;
                let mut best_other = pid as u16;

                // Check 4-neighbors for plate boundary
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

                    // Boundary normal: direction from this cell toward neighbor
                    let nl = (dx as f32).hypot(dy as f32);
                    let normal = [dx as f32 / nl, dy as f32 / nl];

                    // Relative velocity of plates
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

                    if rate > best_rate {
                        best_rate = rate;
                        best_type = bt;
                        best_other = npid as u16;
                    }
                }

                if best_type != INTERIOR {
                    row_boundaries.push((x, best_type, best_rate, pid as u16, best_other));
                }
            }
            (y, row_boundaries)
        })
        .collect();

    // Collect into grids
    for (y, row_data) in rows {
        for (x, bt, _rate, pa, pb) in row_data {
            btype_grid.set(x, y, bt);
            pa_grid.set(x, y, pa);
            pb_grid.set(x, y, pb);
        }
    }

    (btype_grid, pa_grid, pb_grid)
}
