use controller::{Action, Controller};
use eframe::egui;
use studio_math::{Vec2, Vec3};

use super::{ToolSidebar, utils};

/// Height of the normal vector picker panel
const PICKER_HEIGHT: f32 = 150.0;

/// Space between the edge of the picker panel and the ball image
const PICKER_MARGIN: f32 = 10.0;

/// Radius of the circle drawn over the ball at the selected zones' normal
const MARKER_RADIUS: f32 = 4.0;

/// The ball image used to pick normal vectors
#[derive(Default, PartialEq, Copy, Clone)]
enum NormalBall {
    /// A ball split into flat facets, for picking from a fixed set of normals
    #[default]
    Faceted,

    /// A perfectly smooth sphere, for picking any normal
    Smooth,
}

impl NormalBall {
    fn label(self) -> &'static str {
        match self {
            Self::Faceted => "Faceted",
            Self::Smooth => "Smooth",
        }
    }

    fn image(self) -> egui::ImageSource<'static> {
        match self {
            Self::Faceted => egui::include_image!("../../../assets/normal_faceted.png"),
            Self::Smooth => egui::include_image!("../../../assets/normal_smooth.png"),
        }
    }

    /// Get the normal at a position on the ball
    fn normal_at(self, pos: Vec2) -> Vec3 {
        match self {
            Self::Faceted => utils::faceted_normal(pos),
            Self::Smooth => utils::smooth_normal(pos),
        }
    }

    /// Get the position of a normal on the ball, if it's on the ball
    fn position_of(self, normal: Vec3) -> Option<Vec2> {
        match self {
            Self::Faceted => utils::faceted_position(normal),
            Self::Smooth => utils::smooth_position(normal),
        }
    }
}

/// Sidebar options for the zone tool
#[derive(Default)]
pub struct ZoneSidebar {
    /// The ball image used to pick normal vectors
    ball: NormalBall,
}

impl ToolSidebar for ZoneSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, controller: &mut Controller) {
        // Selection info. Everything else applies to the selected zones, so only show it if there are any.
        let zones = controller.selected_zones();
        match zones.len() {
            0 => {
                ui.label("No zones selected.");
                return;
            },
            1 => ui.label(format!("Selected zone: {}", zones[0])),
            count => ui.label(format!("{count} zones selected.")),
        };
        let plural = zones.len() > 1;

        self.show_normal_picker(ui, controller);
        ui.add_space(ui.spacing().item_spacing.y);

        // Zone actions
        let button_size = egui::vec2(ui.available_width(), 0.0);
        let separate = egui::Button::new(if plural { "Separate zones" } else { "Separate zone" }).min_size(button_size);
        if ui.add_enabled(controller.can_separate_selection(), separate).clicked() {
            controller.handle_action(Action::SelectionSeparated);
        }

        let delete = egui::Button::new(if plural { "Delete zones" } else { "Delete zone" }).min_size(button_size);
        if ui.add(delete).clicked() {
            controller.handle_action(Action::Delete);
        }

        // TODO: Clear shapes once they exist
        ui.add(egui::Button::new("Clear shapes").min_size(button_size));
    }
}

impl ZoneSidebar {
    /// Show the ball image for picking the selected zones' normal vector, and the tabs for switching between balls
    fn show_normal_picker(&mut self, ui: &mut egui::Ui, controller: &mut Controller) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // Draw the ball as large as fits in the panel, without stretching
            let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), PICKER_HEIGHT), egui::Sense::click_and_drag());
            ui.painter().rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            let side = (rect.width().min(rect.height()) - 2.0 * PICKER_MARGIN).max(0.0);
            let ball_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(side, side));
            egui::Image::new(self.ball.image()).paint_at(ui, ball_rect);

            // Convert between screen positions and positions on the ball, which are relative to its center with +Y up
            let center = ball_rect.center();
            let radius = side / 2.0;
            let to_ball = |pos: egui::Pos2| Vec2::new((pos.x - center.x) / radius, (center.y - pos.y) / radius);
            let from_ball = |pos: Vec2| center + egui::vec2(pos.x, -pos.y) * radius;

            // Set the normal when pressing or dragging on the panel.
            // Only send it while dragging if it changed, but always send it on press so the error state is updated.
            if radius > 0.0
                && (response.is_pointer_button_down_on() || response.clicked())
                && let Some(pos) = response.interact_pointer_pos() {
                    let normal = self.ball.normal_at(to_ball(pos));
                    let pressed = ui.input(|i| i.pointer.any_pressed());
                    if pressed || controller.selected_zones_normal() != Some(normal) {
                        controller.handle_action(Action::ZoneNormalSet(normal, !pressed));
                    }
                }

            // Mark the selected zones' normal, if they share one and it's on this ball
            if let Some(pos) = controller.selected_zones_normal().and_then(|normal| self.ball.position_of(normal)) {
                let pos = from_ball(pos);
                ui.painter().circle_stroke(pos, MARKER_RADIUS, egui::Stroke::new(3.0, egui::Color32::BLACK));
                ui.painter().circle_stroke(pos, MARKER_RADIUS, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }

            // Tabs for switching between balls, directly below the panel
            ui.horizontal(|ui| {
                let size = egui::vec2(ui.available_width() / 2.0, 20.0);
                for ball in [NormalBall::Faceted, NormalBall::Smooth] {
                    if ui.add_sized(size, egui::Button::selectable(self.ball == ball, ball.label()).corner_radius(0)).clicked() {
                        self.ball = ball;
                    }
                }
            });
        });
    }
}
