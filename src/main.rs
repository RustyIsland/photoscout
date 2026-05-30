#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app;
mod diagnostics;
mod duplicates;
mod error;
mod image_decoders;
mod library_roots;
mod model;
mod path_utils;
mod scan_coordinator;
mod scanner;
mod search;
mod thumbnails;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    init_tracing();
    diagnostics::init_from_env();
    diagnostics::log_runtime_startup();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "PhotoScout",
        options,
        Box::new(|cc| Ok(Box::new(app::PhotoScoutApp::new(cc)))),
    )
    .context("failed to launch PhotoScout")
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,photoscout=info,wgpu_hal=error,egui_wgpu=error".into()),
        )
        .init();
}
