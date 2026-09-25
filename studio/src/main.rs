#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::sync::Arc;

use eframe::{egui};
use eframe::egui_wgpu::{wgpu, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use studio::app::StudioApp;

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // Set up WGPU
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.device_descriptor = Arc::new(|adapter| {
        let base = wgpu::Limits::downlevel_defaults();
        wgpu::DeviceDescriptor {
            label: Some("egui+compute device"),
            required_features: wgpu::Features::empty(),
            required_limits: base.using_resolution(adapter.limits()),
            ..Default::default()
        }
    });

    // Set up the viewport for WGPU
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 540.0]),
        ..Default::default()
    };

    // Return the application
    eframe::run_native(
        "Sprite Normal Studio",
        options,
        Box::new(|cc| {
            // Ensure we are rendering with wgpu
            let render_state = cc.wgpu_render_state.clone().expect("wgpu renderer required");

            // Ensure we have image support
            egui_extras::install_image_loaders(&cc.egui_ctx);

            // Create the application for the window
            Ok(Box::new(StudioApp::new(render_state)))
        })
    )
}