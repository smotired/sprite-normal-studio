#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use studio::app::StudioApp;

fn main() -> eframe::Result {
    // env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // Set up the viewport
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 540.0]),
        ..Default::default()
    };

    // Return the application
    eframe::run_native(
        "Sprite Normal Studio",
        options,
        Box::new(|cc| {
            // Add any extra packages here
            let render_state = cc.wgpu_render_state.clone().expect("wgpu renderer required");

            // Create the application for the window
            Ok(Box::new(StudioApp::new(render_state)))
        })
    )
}