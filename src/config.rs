/// All tunable parameters — exposed as UI sliders in the frontend.
#[derive(Clone, Debug)]
pub struct Params {
    // Plate tectonics
    pub num_plates: usize,
    pub continental_fraction: f32,

    // Elevation profile
    pub blur_sigma: f32,
    pub mountain_scale: f32,
    pub trench_scale: f32,
    pub mountain_width: f32,

    // Noise
    pub coast_amp: f32,
    pub interior_amp: f32,
    pub detail_amp: f32,

    // Features
    pub shelf_width: f32,
    pub ridge_height: f32,
    pub rift_depth: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            num_plates: 12,
            continental_fraction: 0.40,
            blur_sigma: 12.0,
            mountain_scale: 1.0,
            trench_scale: 1.0,
            mountain_width: 50.0,
            coast_amp: 1.0,
            interior_amp: 1.0,
            detail_amp: 50.0,
            shelf_width: 40.0,
            ridge_height: 1500.0,
            rift_depth: 600.0,
        }
    }
}
