pub mod config;
pub mod elevation;
pub mod grid;
pub mod noise;
pub mod plates;
pub mod render;
pub mod rng;

use std::time::Instant;

use config::Params;
use grid::Grid;

pub struct Map {
    pub w: usize,
    pub h: usize,
    pub height: Grid<f32>,
    pub plate_id: Grid<u16>,
    pub boundary_type: Grid<u8>,
    pub boundary_major: Grid<u8>,
    pub boundary_dist: Grid<f32>,
    pub macro_id: Vec<usize>,
    pub num_macro: usize,
    pub rgba: Vec<u8>,
}

pub struct Timing {
    pub name: &'static str,
    pub ms: f64,
}

pub fn generate(seed: u64, w: usize, h: usize, params: &Params) -> (Map, Vec<Timing>) {
    let mut timings = Vec::new();
    let total_start = Instant::now();

    // 1. Seed macroplates first (needed for density-guided microplate seeding)
    let t = Instant::now();
    let macro_seeds = plates::seed::poisson_plate_seeds(
        w, h, params.num_macroplates, seed ^ plates::properties::SALT_MACRO,
    );
    // 2. Seed microplates with variable density: denser near macroplate boundaries
    let seeds = plates::seed::poisson_variable_seeds(
        w, h, params.num_microplates, seed, &macro_seeds,
    );
    timings.push(Timing {
        name: "plate_seed",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    // 3. Grow microplates (noise-weighted Dijkstra)
    let t = Instant::now();
    let plate_id = plates::grow::grow_plates(w, h, &seeds, seed, params.boundary_noise);
    timings.push(Timing {
        name: "plate_grow",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    // 4. Assign plate properties (macro grouping + velocities)
    let t = Instant::now();
    let plate_set = plates::properties::assign_plate_properties(
        params.num_microplates,
        params.num_macroplates,
        &seeds,
        &macro_seeds,
        &plate_id,
        params.continental_fraction,
        params.boundary_noise,
        seed,
    );
    timings.push(Timing {
        name: "plate_properties",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    // 4. Extract + classify boundaries (major/minor)
    let t = Instant::now();
    let (btype_grid, pa_grid, pb_grid, major_grid) =
        plates::boundary::extract_boundaries(&plate_id, &plate_set);
    timings.push(Timing {
        name: "boundaries",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    // 5. Distance field with nearest-boundary propagation
    let t = Instant::now();
    let (dist_grid, near_bx, near_by) =
        plates::distance::boundary_distance_field(&btype_grid);
    timings.push(Timing {
        name: "distance_field",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    // 6. Build elevation from boundary profiles
    let t = Instant::now();
    let height = elevation::build_elevation(
        &plate_id,
        &plate_set,
        &btype_grid,
        &dist_grid,
        &near_bx,
        &near_by,
        &pa_grid,
        &pb_grid,
        &major_grid,
        seed,
        params,
    );
    timings.push(Timing {
        name: "elevation",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    // 7. Render
    let t = Instant::now();
    let rgba = render::render_map(&height);
    timings.push(Timing {
        name: "render",
        ms: t.elapsed().as_secs_f64() * 1000.0,
    });

    let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    timings.push(Timing {
        name: "TOTAL",
        ms: total_ms,
    });

    let map = Map {
        w,
        h,
        height,
        plate_id,
        boundary_type: btype_grid,
        boundary_major: major_grid,
        boundary_dist: dist_grid,
        macro_id: plate_set.macro_id,
        num_macro: plate_set.num_macro,
        rgba,
    };

    (map, timings)
}
