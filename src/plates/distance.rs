use crate::grid::Grid;

/// Squared Euclidean distance from (x,y) to (bx,by) with E-W wrapping.
#[inline]
fn dist_sq(x: usize, y: usize, bx: u16, by: u16, w: usize) -> f32 {
    let dx_raw = (x as f32 - bx as f32).abs();
    let dx = dx_raw.min(w as f32 - dx_raw);
    let dy = y as f32 - by as f32;
    dx * dx + dy * dy
}

/// Euclidean distance field from boundary cells with E-W wrapping.
///
/// Uses Jump Flood Algorithm (JFA) for nearest-boundary propagation.
/// Unlike chamfer sweeps, JFA uses true Euclidean distance comparisons
/// at every step, producing smooth circular contours with no diamond artifacts.
pub fn boundary_distance_field(
    btype: &Grid<u8>,
) -> (Grid<f32>, Grid<u16>, Grid<u16>) {
    let w = btype.w;
    let h = btype.h;
    let n = w * h;

    let mut near_x = vec![u16::MAX; n];
    let mut near_y = vec![u16::MAX; n];

    // Initialize: boundary cells store their own coordinates
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if btype.data[i] != 0 {
                near_x[i] = x as u16;
                near_y[i] = y as u16;
            }
        }
    }

    // Offsets for 8-connected neighbors at a given step
    const DIRS: [(i32, i32); 8] = [
        (-1, 0), (1, 0), (0, -1), (0, 1),
        (-1, -1), (1, -1), (-1, 1), (1, 1),
    ];

    // JFA main passes: step sizes from max_dim/2 down to 1
    let max_dim = w.max(h);
    let mut step = (max_dim.next_power_of_two() / 2) as i32;
    while step >= 1 {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let mut best_sq = if near_x[i] == u16::MAX {
                    f32::MAX
                } else {
                    dist_sq(x, y, near_x[i], near_y[i], w)
                };
                let mut best_bx = near_x[i];
                let mut best_by = near_y[i];

                for &(ddx, ddy) in &DIRS {
                    let ny = y as i32 + ddy * step;
                    if ny < 0 || ny >= h as i32 {
                        continue;
                    }
                    let nx = ((x as i32 + ddx * step) % w as i32 + w as i32) as usize % w;
                    let ni = ny as usize * w + nx;

                    if near_x[ni] == u16::MAX {
                        continue;
                    }

                    let cand = dist_sq(x, y, near_x[ni], near_y[ni], w);
                    if cand < best_sq {
                        best_sq = cand;
                        best_bx = near_x[ni];
                        best_by = near_y[ni];
                    }
                }

                near_x[i] = best_bx;
                near_y[i] = best_by;
            }
        }
        step /= 2;
    }

    // JFA+2 cleanup: extra passes at step=2 and step=1 to fix residual errors
    for extra in [2i32, 1] {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let mut best_sq = if near_x[i] == u16::MAX {
                    f32::MAX
                } else {
                    dist_sq(x, y, near_x[i], near_y[i], w)
                };
                let mut best_bx = near_x[i];
                let mut best_by = near_y[i];

                for &(ddx, ddy) in &DIRS {
                    let ny = y as i32 + ddy * extra;
                    if ny < 0 || ny >= h as i32 {
                        continue;
                    }
                    let nx = ((x as i32 + ddx * extra) % w as i32 + w as i32) as usize % w;
                    let ni = ny as usize * w + nx;

                    if near_x[ni] == u16::MAX {
                        continue;
                    }

                    let cand = dist_sq(x, y, near_x[ni], near_y[ni], w);
                    if cand < best_sq {
                        best_sq = cand;
                        best_bx = near_x[ni];
                        best_by = near_y[ni];
                    }
                }

                near_x[i] = best_bx;
                near_y[i] = best_by;
            }
        }
    }

    // Compute final Euclidean distances from nearest-boundary coordinates
    let dist: Vec<f32> = (0..n)
        .map(|i| {
            if near_x[i] == u16::MAX {
                f32::MAX
            } else {
                dist_sq(i % w, i / w, near_x[i], near_y[i], w).sqrt()
            }
        })
        .collect();

    let dist_grid = Grid { data: dist, w, h };
    let nx_grid = Grid { data: near_x, w, h };
    let ny_grid = Grid { data: near_y, w, h };
    (dist_grid, nx_grid, ny_grid)
}
