use std::net::SocketAddr;

use axum::{Json, Router, routing::post};
use base64::Engine;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;

use worldgen::config::Params;
use worldgen::render;

#[derive(Deserialize)]
struct GenerateRequest {
    seed: Option<u64>,
    width: Option<usize>,
    height: Option<usize>,
    num_plates: Option<usize>,
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
}

#[derive(Serialize)]
struct GenerateResponse {
    layers: Vec<Layer>,
    timings: Vec<TimingEntry>,
    width: usize,
    height: usize,
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

fn encode_png(rgba: &[u8], w: usize, h: usize) -> String {
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8)
        .expect("PNG encode failed");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    format!("data:image/png;base64,{}", b64)
}

async fn generate_handler(Json(req): Json<GenerateRequest>) -> Json<GenerateResponse> {
    let seed = req.seed.unwrap_or(42);
    let width = req.width.unwrap_or(1024);
    let height = req.height.unwrap_or(512);
    let num_plates = req.num_plates.unwrap_or(12);
    let continental_fraction = req.continental_fraction.unwrap_or(0.40);

    let defaults = Params::default();
    let boundary_noise = req.boundary_noise.unwrap_or(defaults.boundary_noise);
    let blur_sigma = req.blur_sigma.unwrap_or(defaults.blur_sigma);
    let mountain_scale = req.mountain_scale.unwrap_or(defaults.mountain_scale);
    let trench_scale = req.trench_scale.unwrap_or(defaults.trench_scale);
    let mountain_width = req.mountain_width.unwrap_or(defaults.mountain_width);
    let coast_amp = req.coast_amp.unwrap_or(defaults.coast_amp);
    let interior_amp = req.interior_amp.unwrap_or(defaults.interior_amp);
    let detail_amp = req.detail_amp.unwrap_or(defaults.detail_amp);
    let shelf_width = req.shelf_width.unwrap_or(defaults.shelf_width);
    let ridge_height = req.ridge_height.unwrap_or(defaults.ridge_height);
    let rift_depth = req.rift_depth.unwrap_or(defaults.rift_depth);

    let response = tokio::task::spawn_blocking(move || {
        let params = Params {
            num_plates,
            continental_fraction,
            boundary_noise,
            blur_sigma,
            mountain_scale,
            trench_scale,
            mountain_width,
            coast_amp,
            interior_amp,
            detail_amp,
            shelf_width,
            ridge_height,
            rift_depth,
        };
        let (map, timings) = worldgen::generate(seed, width, height, &params);

        let layers = vec![
            Layer {
                name: "plates".into(),
                data_url: encode_png(
                    &render::render_plates(&map.plate_id, &map.boundary_type, num_plates),
                    width,
                    height,
                ),
            },
            Layer {
                name: "boundaries".into(),
                data_url: encode_png(
                    &render::render_boundaries(&map.boundary_type),
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
        ];

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

#[tokio::main]
async fn main() {
    let frontend = ServeDir::new("frontend");

    let app = Router::new()
        .route("/api/generate", post(generate_handler))
        .fallback_service(frontend);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    eprintln!("worldgen server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
