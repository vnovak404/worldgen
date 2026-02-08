use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{Json, Router, extract::State, routing::post};
use base64::Engine;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;

use worldgen::config::Params;
use worldgen::render;
use worldgen::Map;

#[derive(Deserialize, Clone)]
struct GenerateRequest {
    seed: Option<u64>,
    width: Option<usize>,
    height: Option<usize>,
    num_macroplates: Option<usize>,
    num_microplates: Option<usize>,
    continental_fraction: Option<f32>,
    boundary_noise: Option<f32>,
    // Elevation profile
    blur_sigma: Option<f32>,
    mountain_scale: Option<f32>,
    trench_scale: Option<f32>,
    mountain_width: Option<f32>,
    // Noise
    coast_amp: Option<f32>,
    interior_amp: Option<f32>,
    detail_amp: Option<f32>,
    // Features
    shelf_width: Option<f32>,
    ridge_height: Option<f32>,
    rift_depth: Option<f32>,
    // Climate / hydrology
    rainfall_scale: Option<f32>,
    river_threshold: Option<f32>,
}

#[derive(Serialize)]
struct GenerateResponse {
    layers: Vec<Layer>,
    timings: Vec<TimingEntry>,
    width: usize,
    height: usize,
}

#[derive(Serialize)]
struct RiversResponse {
    layer: Layer,
    timing: TimingEntry,
}

#[derive(Serialize)]
struct Layer {
    name: String,
    data_url: String,
}

#[derive(Serialize)]
struct TimingEntry {
    name: String,
    ms: f64,
}

/// Shared state: cached base map + generation params for the rivers endpoint.
struct CachedGeneration {
    map: Map,
    seed: u64,
    params: Params,
}

type SharedState = Arc<Mutex<Option<CachedGeneration>>>;

fn encode_png(rgba: &[u8], w: usize, h: usize) -> String {
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8)
        .expect("PNG encode failed");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    format!("data:image/png;base64,{}", b64)
}

fn parse_params(req: &GenerateRequest) -> (u64, usize, usize, Params) {
    let seed = req.seed.unwrap_or(42);
    let width = req.width.unwrap_or(1024);
    let height = req.height.unwrap_or(512);

    let defaults = Params::default();
    let params = Params {
        num_macroplates: req.num_macroplates.unwrap_or(defaults.num_macroplates),
        num_microplates: req.num_microplates.unwrap_or(defaults.num_microplates),
        continental_fraction: req.continental_fraction.unwrap_or(defaults.continental_fraction),
        boundary_noise: req.boundary_noise.unwrap_or(defaults.boundary_noise),
        blur_sigma: req.blur_sigma.unwrap_or(defaults.blur_sigma),
        mountain_scale: req.mountain_scale.unwrap_or(defaults.mountain_scale),
        trench_scale: req.trench_scale.unwrap_or(defaults.trench_scale),
        mountain_width: req.mountain_width.unwrap_or(defaults.mountain_width),
        coast_amp: req.coast_amp.unwrap_or(defaults.coast_amp),
        interior_amp: req.interior_amp.unwrap_or(defaults.interior_amp),
        detail_amp: req.detail_amp.unwrap_or(defaults.detail_amp),
        shelf_width: req.shelf_width.unwrap_or(defaults.shelf_width),
        ridge_height: req.ridge_height.unwrap_or(defaults.ridge_height),
        rift_depth: req.rift_depth.unwrap_or(defaults.rift_depth),
        rainfall_scale: req.rainfall_scale.unwrap_or(defaults.rainfall_scale),
        river_threshold: req.river_threshold.unwrap_or(defaults.river_threshold),
    };

    (seed, width, height, params)
}

/// Fast endpoint: generates everything except hydrology (~2s).
/// Caches the base map so /api/rivers can compute hydrology from it.
async fn generate_handler(
    State(state): State<SharedState>,
    Json(req): Json<GenerateRequest>,
) -> Json<GenerateResponse> {
    let (seed, width, height, params) = parse_params(&req);

    let state_clone = state.clone();
    let response = tokio::task::spawn_blocking(move || {
        let (map, timings) = worldgen::generate_base(seed, width, height, &params);

        let layers = vec![
            Layer {
                name: "plates".into(),
                data_url: encode_png(
                    &render::render_plates(
                        &map.plate_id,
                        &map.boundary_type,
                        &map.boundary_major,
                        &map.macro_id,
                        map.num_macro,
                    ),
                    width,
                    height,
                ),
            },
            Layer {
                name: "boundaries".into(),
                data_url: encode_png(
                    &render::render_boundaries(&map.boundary_type, &map.boundary_major),
                    width,
                    height,
                ),
            },
            Layer {
                name: "distance".into(),
                data_url: encode_png(
                    &render::render_distance(&map.boundary_dist),
                    width,
                    height,
                ),
            },
            Layer {
                name: "heightmap".into(),
                data_url: encode_png(&render::render_heightmap(&map.height), width, height),
            },
            Layer {
                name: "map".into(),
                data_url: encode_png(&map.rgba, width, height),
            },
            Layer {
                name: "temperature".into(),
                data_url: encode_png(
                    &render::render_temperature(&map.temperature),
                    width,
                    height,
                ),
            },
            Layer {
                name: "precipitation".into(),
                data_url: encode_png(
                    &render::render_precipitation(&map.precipitation),
                    width,
                    height,
                ),
            },
        ];

        // Cache the map for rivers endpoint
        *state_clone.lock().unwrap() = Some(CachedGeneration {
            map,
            seed,
            params,
        });

        let timing_entries = timings
            .iter()
            .map(|t| TimingEntry {
                name: t.name.to_string(),
                ms: t.ms,
            })
            .collect();

        GenerateResponse {
            layers,
            timings: timing_entries,
            width,
            height,
        }
    })
    .await
    .unwrap();

    Json(response)
}

/// Slow endpoint: computes hydrology from cached base map (~8s).
/// Carves valleys into the cached heightmap along river paths.
async fn rivers_handler(
    State(state): State<SharedState>,
) -> Json<Option<RiversResponse>> {
    let response = tokio::task::spawn_blocking(move || {
        let mut guard = state.lock().unwrap();
        guard.as_mut().map(|c| {
            let (river_flow, timing) = worldgen::generate_rivers(&mut c.map, c.seed, &c.params);
            let layer = Layer {
                name: "rivers".into(),
                data_url: encode_png(
                    &render::render_rivers(&c.map.height, &river_flow),
                    c.map.w,
                    c.map.h,
                ),
            };
            RiversResponse {
                layer,
                timing: TimingEntry {
                    name: timing.name.to_string(),
                    ms: timing.ms,
                },
            }
        })
    })
    .await
    .unwrap();

    Json(response)
}

#[tokio::main]
async fn main() {
    let frontend = ServeDir::new("frontend");
    let state: SharedState = Arc::new(Mutex::new(None));

    let app = Router::new()
        .route("/api/generate", post(generate_handler))
        .route("/api/rivers", post(rivers_handler))
        .with_state(state)
        .fallback_service(frontend);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    eprintln!("worldgen server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
