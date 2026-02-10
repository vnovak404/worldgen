use std::path::PathBuf;
use worldgen::config::Params;
use worldgen::render;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(42);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2048);
    let height: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let out_dir: PathBuf = args
        .get(4)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts"));

    std::fs::create_dir_all(&out_dir).expect("failed to create output directory");

    let params = Params::default();

    eprintln!(
        "Generating {}x{} map with seed={}, macro={}, micro={}",
        width, height, seed, params.num_macroplates, params.num_microplates
    );

    let (map, timings) = worldgen::generate(seed, width, height, &params);

    // Print timings
    eprintln!("\nTimings:");
    for t in &timings {
        eprintln!("  {:20} {:8.1} ms", t.name, t.ms);
    }

    // Save diagnostic PNGs
    let save = |name: &str, rgba: &[u8], w: usize, h: usize| {
        let path = out_dir.join(name);
        image::save_buffer(&path, rgba, w as u32, h as u32, image::ColorType::Rgba8)
            .expect("failed to save image");
        eprintln!("Saved {}", path.display());
    };

    // 1. Plate map
    let plate_rgba = render::render_plates(
        &map.plate_id,
        &map.boundary_type,
        &map.boundary_major,
        &map.macro_id,
        map.num_macro,
    );
    save("plates.png", &plate_rgba, width, height);

    // 2. Boundary types
    let bound_rgba = render::render_boundaries(&map.boundary_type, &map.boundary_major);
    save("boundaries.png", &bound_rgba, width, height);

    // 3. Distance field
    let dist_rgba = render::render_distance(&map.boundary_dist);
    save("distance.png", &dist_rgba, width, height);

    // 4. Grayscale heightmap
    let hmap_rgba = render::render_heightmap(&map.height);
    save("heightmap.png", &hmap_rgba, width, height);

    // 5. Final rendered map
    save("map.png", &map.rgba, width, height);

    // 6. Temperature
    let temp_rgba = render::render_temperature(&map.temperature);
    save("temperature.png", &temp_rgba, width, height);

    // 7. Precipitation
    let precip_rgba = render::render_precipitation(&map.precipitation);
    save("precipitation.png", &precip_rgba, width, height);

    // 8. Rivers
    let river_rgba = render::render_rivers(&map.height, &map.river_flow, &map.precipitation, &map.temperature);
    save("rivers.png", &river_rgba, width, height);

    eprintln!("\nDone.");
}
