use std::path::Path;

use eframe::egui;

use crate::app::{file, workspace, StudioApp};
use crate::app::workspace::WorkspaceCommand;

/// Space between a tab's edges and its contents
const TAB_PADDING: f32 = 8.0;

/// Something clicked in the tab bar, which is run after the bar is shown since the bar borrows the app
enum TabAction {
    Select(usize),
    Close(usize),
    Command(WorkspaceCommand),
}

/// What was clicked on a single tab
enum TabClick {
    None,
    Select,
    Close,
}

impl StudioApp {
    /// Render the tab bar below the menu bar while any project is open: a tab for each project, scrolling if they
    /// don't fit, and buttons to open or create another project against the right edge.
    /// Returns the workspace command that was clicked, if any. Everything else clicked is run here.
    pub(super) fn show_tab_bar(&mut self, ui: &mut egui::Ui) -> Option<WorkspaceCommand> {
        if self.projects.is_empty() { return None; }

        let mut clicked = None;
        let scroll_to_current = std::mem::take(&mut self.scroll_to_current);
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                // Added right to left, so N is against the edge
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // TODO: Icons
                    if ui.button("N").on_hover_text("New Project").clicked() {
                        clicked = Some(TabAction::Command(WorkspaceCommand::New));
                    }
                    if ui.button("O").on_hover_text("Open").clicked() {
                        clicked = Some(TabAction::Command(WorkspaceCommand::Open));
                    }

                    // The tabs take the rest of the row
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (index, project) in self.projects.iter().enumerate() {
                                let selected = index == self.current;
                                let title = workspace::tab_title(project);
                                match tab(ui, index, &title, &project.path, selected, selected && scroll_to_current) {
                                    TabClick::Select => clicked = Some(TabAction::Select(index)),
                                    TabClick::Close => clicked = Some(TabAction::Close(index)),
                                    TabClick::None => {},
                                }
                            }
                        });
                    });
                });
            });
        });

        let ctx = ui.ctx().clone();
        match clicked? {
            TabAction::Select(index) => self.activate(&ctx, index),
            TabAction::Close(index) => {
                if self.confirm_discard(&ctx, index) {
                    self.close_project(&ctx, index);
                }
            },
            TabAction::Command(command) => return Some(command),
        }
        None
    }
}

/// A single tab with the project's name, and a cross to close it that is shown while the tab is hovered.
/// The cross always takes up its space, so tabs don't change width when hovered.
fn tab(ui: &mut egui::Ui, index: usize, title: &str, path: &Path, selected: bool, scroll_to: bool) -> TabClick {
    let id = ui.id().with(("tab", index));
    let galley = egui::WidgetText::from(title)
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
    let cross_size = ui.spacing().interact_size.y;
    let size = egui::vec2(TAB_PADDING + galley.size().x + 2.0 + cross_size + TAB_PADDING / 2.0, cross_size + 4.0);

    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let cross_rect = egui::Rect::from_center_size(
        egui::pos2(rect.max.x - TAB_PADDING / 2.0 - cross_size / 2.0, rect.center().y),
        egui::Vec2::splat(cross_size),
    );
    // The cross is added after the tab, so it gets the clicks over it
    let hovered = ui.rect_contains_pointer(rect);
    let close = file::close_button_at(ui, cross_rect, id.with("close"), hovered);
    if scroll_to {
        response.scroll_to_me(None);
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if selected {
            visuals.widgets.active.weak_bg_fill
        } else if hovered {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        let corner_radius = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
        ui.painter().rect_filled(rect, corner_radius, fill);

        let text_color = if selected { visuals.strong_text_color() } else { visuals.text_color() };
        let text_pos = egui::pos2(rect.min.x + TAB_PADDING, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, text_color);
    }

    if close.clicked() {
        TabClick::Close
    } else if response.on_hover_text(path.display().to_string()).clicked() {
        TabClick::Select
    } else {
        TabClick::None
    }
}
