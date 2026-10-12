use controller::Controller;
use eframe::egui;

use super::ToolSidebar;

/// Sidebar options for the point tool
#[derive(Default)]
pub struct PointSidebar;

impl ToolSidebar for PointSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, _controller: &mut Controller) {
        ui.label("Using Point Tool");
    }
}
