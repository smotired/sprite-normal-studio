use eframe::egui;

use crate::app::StudioApp;

/// Widest the welcome area gets
const WIDTH: f32 = 420.0;

/// Space around the welcome area, at least
const MARGIN: f32 = 32.0;

impl StudioApp {
    /// Render the Welcome screen, shown when no project is open: a button to create a new project and the recent projects
    pub(super) fn show_welcome(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            // A narrow column in the middle of the window, a little above center, with empty space all around it
            let full = ui.max_rect();
            let width = WIDTH.min(full.width() - 2.0 * MARGIN).max(0.0);
            let top = (full.height() * 0.15).max(MARGIN);
            let column = egui::Rect::from_min_max(
                egui::pos2(full.center().x - width / 2.0, full.min.y + top),
                egui::pos2(full.center().x + width / 2.0, full.max.y - MARGIN),
            );

            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                egui::Frame::group(ui.style()).inner_margin(24).show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.vertical_centered(|ui| {
                        ui.heading(egui::RichText::new("Sprite Normal Studio").size(28.0));
                        ui.add_space(16.0);
                        if ui.add(egui::Button::new("New Project").min_size(egui::vec2(160.0, 32.0))).clicked() {
                            self.new_project_dialog = Some(Default::default());
                        }
                    });

                    ui.add_space(24.0);
                    ui.heading("Recent Projects");
                    ui.separator();
                    self.show_recent_projects(ui);
                });
            });
        });
    }

    /// The recent projects list. Clicking one opens it, and the cross removes it from the list.
    fn show_recent_projects(&mut self, ui: &mut egui::Ui) {
        if self.recent.paths().is_empty() {
            ui.label(egui::RichText::new("No recent projects").weak());
            return;
        }

        // Act after the list is shown, since both change it
        let mut open = None;
        let mut remove = None;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for path in self.recent.paths() {
                ui.horizontal(|ui| {
                    let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
                    if ui.link(name).on_hover_text(path.display().to_string()).clicked() {
                        open = Some(path.clone());
                    }

                    // The folder fills the rest of the row, cut off if it's too long
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if remove_button(ui).on_hover_text("Remove from recent projects").clicked() {
                            remove = Some(path.clone());
                        }
                        let folder = path.parent().map(|folder| folder.display().to_string()).unwrap_or_default();
                        ui.add(egui::Label::new(egui::RichText::new(folder).weak()).truncate());
                    });
                });
            }
        });

        if let Some(path) = remove {
            self.recent.remove(&path);
        }
        if let Some(path) = open {
            self.open_recent(&ui.ctx().clone(), &path);
        }
    }
}

/// A small button with a cross, drawn instead of using a glyph so it doesn't depend on the font
fn remove_button(ui: &mut egui::Ui) -> egui::Response {
    let size = egui::Vec2::splat(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() || response.has_focus() {
            ui.painter().rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
        }
        let cross = rect.shrink(rect.width() * 0.3);
        let stroke = egui::Stroke::new(1.5, visuals.fg_stroke.color);
        ui.painter().line_segment([cross.left_top(), cross.right_bottom()], stroke);
        ui.painter().line_segment([cross.right_top(), cross.left_bottom()], stroke);
    }
    response
}
