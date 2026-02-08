use std::collections::VecDeque;

use crate::grid::{Grid, neighbors4_wrap};
use crate::rng::Rng;

/// Grow plates via randomized round-robin BFS from seed positions.
/// Produces irregular, organic plate shapes (NOT convex Voronoi).
pub fn grow_plates(w: usize, h: usize, seeds: &[[f32; 2]], seed: u64) -> Grid<u16> {
    let mut plate_id = Grid::<u16>::new(w, h);
    for v in &mut plate_id.data {
        *v = u16::MAX;
    }

    let num = seeds.len();
    let mut frontiers: Vec<VecDeque<(usize, usize)>> = Vec::with_capacity(num);
    let mut rng = Rng::new(seed ^ 0xBF5_0001_CAFE_0001);

    // Seed each plate
    for (i, s) in seeds.iter().enumerate() {
        let x = (s[0] as usize).min(w - 1);
        let y = (s[1] as usize).min(h - 1);
        let mut q = VecDeque::new();
        if plate_id.get(x, y) == u16::MAX {
            plate_id.set(x, y, i as u16);
            q.push_back((x, y));
        }
        frontiers.push(q);
    }

    // Round-robin BFS: each plate pops 1-3 cells per turn
    let mut active = num;
    while active > 0 {
        active = 0;
        for pi in 0..num {
            if frontiers[pi].is_empty() {
                continue;
            }
            let pops = 1 + (rng.next_u32() % 3) as usize;
            for _ in 0..pops {
                let Some((cx, cy)) = frontiers[pi].pop_front() else {
                    break;
                };
                for (nx, ny) in neighbors4_wrap(cx, cy, w, h) {
                    if plate_id.get(nx, ny) == u16::MAX {
                        plate_id.set(nx, ny, pi as u16);
                        frontiers[pi].push_back((nx, ny));
                    }
                }
            }
            if !frontiers[pi].is_empty() {
                active += 1;
            }
        }
    }

    plate_id
}

