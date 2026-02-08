use std::collections::BinaryHeap;

use crate::grid::{Grid, neighbors8_wrap};
use crate::noise::fbm;
use crate::rng::seed_u32;

const SALT_GROW: u64 = 0x6120_7700_CAFE_0002;

/// Priority queue entry for noise-weighted Voronoi growth.
/// Implements Ord with reversed cost for min-heap behavior.
#[derive(PartialEq)]
struct Entry {
    cost: f32,
    x: usize,
    y: usize,
    pid: u16,
}

impl Eq for Entry {}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse: lowest cost pops first (min-heap from max-heap)
        other.cost.total_cmp(&self.cost)
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Grow plates via noise-weighted Dijkstra expansion from seed positions.
///
/// Each cell's growth cost is modulated by multi-octave Perlin noise,
/// so plate boundaries follow noise contours instead of straight Voronoi edges.
/// `boundary_noise` controls how much boundaries deviate: 0 = straight, higher = more organic.
pub fn grow_plates(
    w: usize,
    h: usize,
    seeds: &[[f32; 2]],
    seed: u64,
    boundary_noise: f32,
) -> Grid<u16> {
    let mut plate_id = Grid::<u16>::new(w, h);
    for v in &mut plate_id.data {
        *v = u16::MAX;
    }

    let noise_seed = seed_u32(seed, SALT_GROW);
    let mut heap = BinaryHeap::new();

    // Seed each plate at cost 0 — don't claim yet, claim on pop
    for (i, s) in seeds.iter().enumerate() {
        let x = (s[0] as usize).min(w - 1);
        let y = (s[1] as usize).min(h - 1);
        heap.push(Entry {
            cost: 0.0,
            x,
            y,
            pid: i as u16,
        });
    }

    // Multi-source Dijkstra: lowest-cost plate to reach a cell claims it.
    // Cells are claimed on POP (not push) so the noise-weighted cost
    // actually determines boundary placement.
    while let Some(Entry { cost, x, y, pid }) = heap.pop() {
        // Skip if already claimed — someone got here cheaper
        if plate_id.get(x, y) != u16::MAX {
            continue;
        }
        plate_id.set(x, y, pid); // Claim on pop = lowest cost wins

        for (nx, ny) in neighbors8_wrap(x, y, w, h) {
            if plate_id.get(nx, ny) != u16::MAX {
                continue;
            }

            // Step distance: 1.0 cardinal, sqrt(2) diagonal
            let x_moved = nx != x;
            let y_moved = ny != y;
            let step = if x_moved && y_moved { 1.414 } else { 1.0 };

            // Noise-weighted cost: FBM sampled at cell position.
            // The noise field creates "hills" that slow growth and "valleys"
            // that speed it up, so boundaries follow noise contours.
            let u = nx as f32 / w as f32;
            let v = ny as f32 / h as f32;
            let noise = fbm(u, v, noise_seed, 4, 6.0, 2.0, 0.5);
            let cost_mult = (1.0 + noise * boundary_noise).max(0.05);

            let new_cost = cost + step * cost_mult;
            heap.push(Entry {
                cost: new_cost,
                x: nx,
                y: ny,
                pid,
            });
        }
    }

    plate_id
}
