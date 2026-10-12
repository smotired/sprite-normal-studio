use controller::Controller;
use eframe::egui;

use super::ToolSidebar;

/// Sidebar options for the pen tool
#[derive(Default)]
pub struct PenSidebar;

impl ToolSidebar for PenSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, _controller: &mut Controller) {
        ui.label("Using Pen Tool");
    }
}
