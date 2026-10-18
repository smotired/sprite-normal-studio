use controller::{Action, Animation, Controller, Frame, cell_of, next_cell, origin_of};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, TextureId};
use eframe::wgpu::FilterMode;
use rendering::{PreviewUniform, preview_inv_scale};
use studio_math::Vec2;

use crate::app::StudioApp;

/// Height of the timeline panel when it is first opened, in points. It can be resized to any height.
const DEFAULT_HEIGHT: f32 = 320.0;

/// Smallest size of the square each frame's thumbnail fits in, in points. Thumbnails grow to fill the panel's height.
const MIN_THUMBNAIL_SIZE: f32 = 48.0;

/// Width of the outline around the selected frame's thumbnail, in points
const SELECTED_OUTLINE: f32 = 3.0;

/// Size of the button that adds a frame, in points
const ADD_BUTTON_SIZE: f32 = 28.0;

/// Background behind thumbnails, the same gray as behind transparent pixels in the viewport (GRAY_BACKGROUND in spritesheet.wgsl)
const THUMBNAIL_BACKGROUND: Color32 = Color32::from_gray(64);

/// Background of the preview when there is nothing to show, the same gray as outside the spritesheet in the viewport (GRAY_OOB)
const EMPTY_PREVIEW_BACKGROUND: Color32 = Color32::from_gray(51);

/// Height of the light in the preview to start with, and how far PageUp and PageDown move it, like the main light
const DEFAULT_LIGHT_HEIGHT: f32 = 200.0;
const LIGHT_HEIGHT_STEP: f32 = 100.0;
const MIN_LIGHT_HEIGHT: f32 = 100.0;

/// How close to the light thumb, in points, dragging grabs it. The thumb's radius in preview.wgsl, plus 3 points.
const LIGHT_GRAB_RADIUS: f32 = 9.0;

/// How far outside the sprite the light can be dragged, as a fraction of the sprite's size
const LIGHT_MARGIN: f32 = 0.25;

/// How far the preview can zoom in and out from fitting the sprite, in powers of 2. The same range as the camera.
const ZOOM_RANGE: std::ops::RangeInclusive<i32> = -2..=5;

/// Longest hold a frame can have
const MAX_HOLD: u32 = 99;

/// The timeline of a project: which animation and frame are selected, and how the preview is shown. Not saved.
pub(super) struct TimelineState {
    selected_animation: Option<usize>,
    selected_frame: Option<usize>,

    /// If the preview is playing the animation, and the time it started playing at
    playing: bool,
    play_start: f64,

    /// The new name of the selected animation while it is being renamed, and if the name field still needs focus
    rename: Option<String>,
    focus_rename: bool,

    /// If deleting the selected animation is waiting to be confirmed
    confirm_delete: bool,

    /// If the next click in the workspace picks the selected frame's sprite
    picking: bool,

    /// Zoom in powers of 2 from fitting the sprite
    zoom: i32,

    /// The preview's light, relative to the top left of the sprite. None puts it in the middle of the sprite.
    light_offset: Option<Vec2>,
    light_height: f32,
    dragging_light: bool,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            selected_animation: None,
            selected_frame: None,
            playing: false,
            play_start: 0.0,
            rename: None,
            focus_rename: false,
            confirm_delete: false,
            picking: false,
            zoom: 0,
            light_offset: None,
            light_height: DEFAULT_LIGHT_HEIGHT,
            dragging_light: false,
        }
    }
}

impl TimelineState {
    /// If the next click in the workspace picks the selected frame's sprite
    pub(super) fn picking(&self) -> bool { self.picking }

    /// Stop picking a sprite in the workspace
    pub(super) fn cancel_pick(&mut self) { self.picking = false; }

    /// Select the frame before or after the selected one, stopping at the ends, and center the camera on it
    pub(super) fn step_frame(&mut self, controller: &mut Controller, forward: bool) {
        let Some(index) = self.selected_animation.filter(|&index| index < controller.animations().len()) else { return; };
        let count = controller.animations()[index].frames.len();
        if count == 0 { return; }
        let current = self.selected_frame.unwrap_or(0);
        let next = if forward { (current + 1).min(count - 1) } else { current.saturating_sub(1) };
        select_frame(self, controller, index, Some(next));
    }

    /// Start or stop playing the selected animation, if it has frames
    pub(super) fn toggle_play(&mut self, controller: &Controller, now: f64) {
        let has_frames = self.selected_animation.and_then(|index| controller.animations().get(index)).is_some_and(|animation| !animation.frames.is_empty());
        if !has_frames && !self.playing { return; }
        self.playing = !self.playing;
        self.play_start = now;
    }

    /// Start or stop picking a sprite in the workspace for the selected frame. Does nothing if no frame is selected to pick for.
    pub(super) fn toggle_pick(&mut self) {
        if self.selected_frame.is_some() { self.picking = !self.picking; }
    }

    /// Show the sprite at a world space position in the selected frame, and stop picking.
    /// Positions outside every whole sprite are ignored, and picking continues.
    pub(super) fn pick(&mut self, controller: &mut Controller, pos: Vec2) {
        let (Some(index), Some(frame_index)) = (self.selected_animation, self.selected_frame) else { return; };
        let Some(cell) = cell_at(pos, controller.sprite_size(), controller.sheet_cells()) else { return; };
        let sprite_size = controller.sprite_size();
        let Some(frame) = controller.animations_mut().get_mut(index).and_then(|animation| animation.frames.get_mut(frame_index)) else { return; };
        frame.origin = origin_of(cell, sprite_size);
        self.picking = false;
        controller.set_feedback(format!("Frame {} shows column {}, row {}", frame_index + 1, cell.0, cell.1));
    }
}

/// The top left corner and size of the whole sprite at a world space position, if there is one there
pub(super) fn sprite_rect_at(controller: &Controller, pos: Vec2) -> Option<(Vec2, Vec2)> {
    let sprite_size = controller.sprite_size();
    let cell = cell_at(pos, sprite_size, controller.sheet_cells())?;
    let origin = origin_of(cell, sprite_size);
    Some((Vec2::new(origin.0 as f32, origin.1 as f32), Vec2::new(sprite_size.0 as f32, sprite_size.1 as f32)))
}

/// The cell of the sprite grid at a world space position, if it is in one of the whole sprites
fn cell_at(pos: Vec2, sprite_size: (u32, u32), (columns, rows): (u32, u32)) -> Option<(u32, u32)> {
    if pos.x < 0.0 || pos.y < 0.0 || sprite_size.0 == 0 || sprite_size.1 == 0 { return None; }
    let cell = ((pos.x / sprite_size.0 as f32) as u32, (pos.y / sprite_size.1 as f32) as u32);
    (cell.0 < columns && cell.1 < rows).then_some(cell)
}

/// What the preview should draw this frame. It's laid out with the panel, but drawn after the normal map is updated.
pub(super) struct PreviewRequest {
    uniform: PreviewUniform,

    /// Size of the preview in points
    size: egui::Vec2,
}

/// Something done to the frames of the selected animation, applied after they are all shown
enum FrameEdit {
    Select(usize),
    Duplicate(usize),
    Remove(usize),
    Move(usize, bool),
    Add,
}

impl StudioApp {
    /// Render the timeline panel below the viewport. Must be shown after the status bar so it sits directly above it.
    /// Returns the panel's response so the viewport can be placed above it.
    pub(super) fn show_timeline(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let preview_texture = self.preview_texture_id;
        let project = self.projects.get_mut(self.current);
        let response = egui::Panel::bottom("timeline")
            .resizable(true)
            .default_size(DEFAULT_HEIGHT)
            .show(ui, |ui| {
                let project = project?;
                let sprite = (project.sprite.texture_id(), project.sprite.size());
                timeline_ui(ui, &mut project.timeline, &mut project.controller, sprite, preview_texture)
            });
        self.pending_preview = response.inner;
        response.response
    }

    /// Draw the preview laid out by the timeline this frame, if any. Must run after the controller updates the normal map.
    pub(super) fn render_preview(&mut self, ctx: &egui::Context) {
        let Some(request) = self.pending_preview.take() else { return; };
        let ppp = ctx.pixels_per_point();
        let size = Vec2::from(request.size) * ppp;
        let device = &self.render_state.device;
        let view = self.preview_renderer.render(device, &self.render_state.queue, &request.uniform, size, ppp);
        self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(device, view, FilterMode::Nearest, self.preview_texture_id);
    }
}

/// The contents of the timeline panel: the preview on the left, then the animation's controls and frames.
/// Returns what the preview should draw, if there is a frame to show.
fn timeline_ui(ui: &mut egui::Ui, state: &mut TimelineState, controller: &mut Controller, sprite: (TextureId, (u32, u32)), preview_texture: TextureId) -> Option<PreviewRequest> {
    // Keep the selection in range of the animations, which loading or undoing can't change but deleting can
    let count = controller.animations().len();
    state.selected_animation = match state.selected_animation {
        _ if count == 0 => None,
        Some(index) => Some(index.min(count - 1)),
        None => Some(0),
    };
    let frame_count = state.selected_animation.map_or(0, |index| controller.animations()[index].frames.len());
    // A frame is always selected while there are any, so its controls are shown
    state.selected_frame = state.selected_frame.filter(|&index| index < frame_count).or((frame_count > 0).then_some(0));

    if state.selected_frame.is_none() { state.picking = false; }

    // The preview is laid out before the frames, so if they change it has to be drawn again to show them
    let before = (state.selected_animation, state.selected_frame, state.selected_animation.map(|index| controller.animations()[index].frames.clone()));

    // Controls along the top, then the preview as a square filling the rest of the height, and the frames beside it
    let now = ui.input(|i| i.time);
    animation_controls(ui, state, controller, now);
    ui.separator();
    let side = ui.available_height().max(MIN_THUMBNAIL_SIZE);
    let request = ui.horizontal_top(|ui| {
        let request = preview_ui(ui, state, controller, preview_texture, side, now);
        ui.separator();
        // Capped to the preview's height, so the frames can never make the panel taller than it was sized to
        egui::ScrollArea::vertical().id_salt("frame_column").max_height(side).auto_shrink([false, true]).show(ui, |ui| ui.vertical(|ui| {
            if let Some(index) = state.selected_animation {
                // Thumbnails fill the height left by their labels, the scroll bar, and the selected frame's controls
                let spacing = ui.spacing();
                let reserved = 2.0 * spacing.interact_size.y + 4.0 * spacing.item_spacing.y + spacing.scroll.bar_width + spacing.scroll.bar_outer_margin;
                let thumbnail = (side - reserved).max(MIN_THUMBNAIL_SIZE);
                frame_strip(ui, state, controller, index, sprite, thumbnail);
                selected_frame_controls(ui, state, controller, index);
            }
        }));
        request
    }).inner;

    let after = (state.selected_animation, state.selected_frame, state.selected_animation.map(|index| controller.animations()[index].frames.clone()));
    if after != before { ui.ctx().request_repaint(); }
    request
}

/// The lit preview of the animation, a square with a draggable light that zooms with the wheel
fn preview_ui(ui: &mut egui::Ui, state: &mut TimelineState, controller: &Controller, preview_texture: TextureId, side: f32, now: f64) -> Option<PreviewRequest> {
    let animation = state.selected_animation.and_then(|index| controller.animations().get(index));

    // The frame to show: the one playing, or the selected one, or the first
    let shown = animation.and_then(|animation| match state.playing {
        true => animation.frame_at(now - state.play_start),
        false => state.selected_frame.or(Some(0)).filter(|&index| index < animation.frames.len()),
    });
    if state.playing && let Some(animation) = animation {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f32(1.0 / animation.fps.max(1.0)));
    }

    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), Sense::click_and_drag());
    match (animation, shown) {
        (Some(animation), Some(index)) => {
            let (min, size) = controller.frame_rect(&animation.frames[index]);
            let request = preview_interaction(ui, state, &response, rect, min, size, controller);
            ui.painter().image(preview_texture, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            Some(request)
        },
        _ => {
            ui.painter().rect_filled(rect, 0.0, EMPTY_PREVIEW_BACKGROUND);
            let text = if animation.is_some() { "No frames" } else { "No animation" };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::default(), ui.visuals().weak_text_color());
            None
        },
    }
}

/// Drag the light, change its height and zoom the preview, then build what the preview should draw for a sprite
fn preview_interaction(ui: &egui::Ui, state: &mut TimelineState, response: &egui::Response, rect: Rect, min: Vec2, size: Vec2, controller: &Controller) -> PreviewRequest {
    let inv_scale = preview_inv_scale(size, Vec2::from(rect.size()), state.zoom);
    let mut offset = state.light_offset.unwrap_or(size * 0.5);
    let to_local = |pos: Pos2| size * 0.5 + Vec2::from(pos - rect.center()) * inv_scale;

    // Grab the light if the drag starts on its thumb, which is a fixed size on screen
    if response.drag_started() && let Some(pos) = response.interact_pointer_pos() {
        let thumb = (offset - size * 0.5) * (1.0 / inv_scale);
        state.dragging_light = Vec2::from(pos - rect.center()).distance(thumb) <= LIGHT_GRAB_RADIUS;
    }
    if state.dragging_light && response.dragged() && let Some(pos) = response.interact_pointer_pos() {
        offset = clamp_light_offset(to_local(pos), size);
        state.light_offset = Some(offset);
    }
    if response.drag_stopped() {
        state.dragging_light = false;
    }

    // PageUp and PageDown change the height while dragging, like the main light
    if state.dragging_light {
        let (up, down) = ui.input(|i| (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)));
        if up { state.light_height += LIGHT_HEIGHT_STEP; }
        if down { state.light_height = (state.light_height - LIGHT_HEIGHT_STEP).max(MIN_LIGHT_HEIGHT); }
    }

    // The wheel zooms around the center. The preview never pans.
    if response.hovered() {
        let scroll = ui.input(|i| i.events.iter().filter_map(|event| match event {
            egui::Event::MouseWheel { delta, .. } => Some(delta.y),
            _ => None,
        }).sum::<f32>());
        if scroll != 0.0 {
            state.zoom = (state.zoom + scroll.signum() as i32).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        }
    }

    let flags = controller.normals_on() as u32 | (controller.lighting_on() as u32) << 1 | (state.dragging_light as u32) << 2;
    PreviewRequest {
        uniform: PreviewUniform {
            sprite_min: min,
            sprite_size: size,
            light_offset: offset,
            inv_scale: preview_inv_scale(size, Vec2::from(rect.size()), state.zoom),
            light_height: state.light_height,
            light_color: controller.light_color(),
            flags,
            ..Default::default()
        },
        size: rect.size(),
    }
}

/// Playing the animation, choosing, adding, renaming and deleting animations, and setting the selected one's FPS
fn animation_controls(ui: &mut egui::Ui, state: &mut TimelineState, controller: &mut Controller, now: f64) {
    ui.horizontal(|ui| {
        let can_play = state.selected_animation.is_some_and(|index| !controller.animations()[index].frames.is_empty());
        let label = if state.playing { "Pause" } else { "Play" };
        if ui.add_enabled(can_play, egui::Button::new(label)).on_hover_text("Play or pause (Space)").clicked() {
            state.toggle_play(controller, now);
        }
        ui.separator();

        let names: Vec<String> = controller.animations().iter().map(|animation| animation.name.clone()).collect();
        let selected_name = state.selected_animation.map_or("No animations", |index| names[index].as_str());
        egui::ComboBox::from_id_salt("animation").selected_text(selected_name).show_ui(ui, |ui| {
            for (index, name) in names.iter().enumerate() {
                if ui.selectable_label(state.selected_animation == Some(index), name).clicked() && state.selected_animation != Some(index) {
                    select_animation(state, Some(index));
                }
            }
        });

        if ui.button("New").clicked() {
            let name = unique_name(&names);
            controller.animations_mut().push(Animation::new(name.clone()));
            select_animation(state, Some(names.len()));
            state.rename = Some(name);
            state.focus_rename = true;
        }

        let Some(index) = state.selected_animation else { return; };

        // Renaming, applied when the field loses focus unless escape was pressed
        if let Some(name) = &mut state.rename {
            let response = ui.add(egui::TextEdit::singleline(name).desired_width(100.0));
            if state.focus_rename {
                response.request_focus();
                state.focus_rename = false;
            }
            if response.lost_focus() {
                let name = name.trim().to_owned();
                if !ui.input(|i| i.key_pressed(Key::Escape)) && !name.is_empty() {
                    controller.animations_mut()[index].name = name;
                }
                state.rename = None;
            }
        } else if ui.button("Rename").clicked() {
            state.rename = Some(names[index].clone());
            state.focus_rename = true;
        }

        // Deleting, after confirming
        if state.confirm_delete {
            ui.label(format!("Delete \"{}\"?", names[index]));
            if ui.button("Delete").clicked() {
                controller.animations_mut().remove(index);
                select_animation(state, index.checked_sub(1).or((names.len() > 1).then_some(0)));
            }
            if ui.button("Cancel").clicked() {
                state.confirm_delete = false;
            }
        } else if ui.button("Delete").clicked() {
            state.confirm_delete = true;
        }

        ui.separator();
        ui.label("FPS");
        let fps = &mut controller.animations_mut()[index].fps;
        ui.add(egui::DragValue::new(fps).range(1.0..=60.0).speed(0.1).max_decimals(1));

        ui.separator();
        ui.label(egui::RichText::new(format!("Preview zoom {}", zoom_label(state.zoom)))).on_hover_text("Scroll over the preview to zoom");
    });
}

/// Select an animation, starting it from its first frame and stopping it from playing
fn select_animation(state: &mut TimelineState, index: Option<usize>) {
    state.selected_animation = index;
    state.selected_frame = None;
    state.playing = false;
    state.rename = None;
    state.confirm_delete = false;
    state.picking = false;
}

/// The frames of an animation as thumbnails, with a button to add the next sprite
fn frame_strip(ui: &mut egui::Ui, state: &mut TimelineState, controller: &mut Controller, index: usize, (sprite_texture, sheet): (TextureId, (u32, u32)), thumbnail: f32) {
    let frames = controller.animations()[index].frames.clone();
    let sheet = Vec2::new(sheet.0.max(1) as f32, sheet.1.max(1) as f32);
    let mut edit = None;
    let playing = state.playing.then(|| controller.animations()[index].frame_at(ui.input(|i| i.time) - state.play_start)).flatten();

    // The wheel scrolls the frames sideways without holding shift, since that's the only way they scroll.
    // The style only applies inside this scope, so other scroll areas aren't affected.
    ui.scope(|ui| {
        ui.style_mut().always_scroll_the_only_direction = true;
        egui::ScrollArea::horizontal().id_salt("frames").show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for (frame_index, frame) in frames.iter().enumerate() {
                    ui.vertical(|ui| {
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(thumbnail, thumbnail), Sense::click());
                        let selected = state.selected_frame == Some(frame_index);

                        // The sprite, fit in the thumbnail's square, on the viewport's background
                        let (min, size) = controller.frame_rect(frame);
                        let inner = rect.shrink(SELECTED_OUTLINE);
                        let scale = inner.width() / size.x.max(size.y).max(1.0);
                        let image_rect = Rect::from_center_size(rect.center(), egui::vec2(size.x * scale, size.y * scale));
                        let uv = Rect::from_min_size(Pos2::new(min.x / sheet.x, min.y / sheet.y), egui::vec2(size.x / sheet.x, size.y / sheet.y));
                        let visuals = ui.visuals();
                        ui.painter().rect_filled(rect, 2.0, THUMBNAIL_BACKGROUND);
                        ui.painter().image(sprite_texture, image_rect, uv, Color32::WHITE);

                        // A thick outline on the selected frame, and a thin one on the frame playing
                        if selected {
                            let stroke = egui::Stroke::new(SELECTED_OUTLINE, visuals.selection.stroke.color);
                            ui.painter().rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
                        } else if playing == Some(frame_index) {
                            ui.painter().rect_stroke(rect, 2.0, visuals.widgets.inactive.fg_stroke, egui::StrokeKind::Inside);
                        } else if response.hovered() {
                            ui.painter().rect_stroke(rect, 2.0, visuals.widgets.hovered.bg_stroke, egui::StrokeKind::Inside);
                        }
                        let label = egui::RichText::new(format!("{} ×{}", frame_index + 1, frame.hold));
                        ui.label(if selected { label.strong().color(visuals.selection.stroke.color) } else { label });

                        if response.clicked() {
                            edit = Some(FrameEdit::Select(frame_index));
                        }
                        response.context_menu(|ui| {
                            if ui.button("Duplicate").clicked() { edit = Some(FrameEdit::Duplicate(frame_index)); }
                            if ui.button("Remove").clicked() { edit = Some(FrameEdit::Remove(frame_index)); }
                            if ui.add_enabled(frame_index > 0, egui::Button::new("Move Left")).clicked() { edit = Some(FrameEdit::Move(frame_index, false)); }
                            if ui.add_enabled(frame_index + 1 < frames.len(), egui::Button::new("Move Right")).clicked() { edit = Some(FrameEdit::Move(frame_index, true)); }
                        });
                    });
                }

                if ui.add_sized([ADD_BUTTON_SIZE, ADD_BUTTON_SIZE], egui::Button::new("+")).on_hover_text("Add the next sprite").clicked() {
                    edit = Some(FrameEdit::Add);
                }
            });
        });
    });

    let Some(edit) = edit else { return; };
    let (sprite_size, cells) = (controller.sprite_size(), controller.sheet_cells());
    let frames = &mut controller.animations_mut()[index].frames;
    let selected = match edit {
        FrameEdit::Select(frame_index) => Some(frame_index),
        FrameEdit::Duplicate(frame_index) => {
            frames.insert(frame_index + 1, frames[frame_index]);
            Some(frame_index + 1)
        },
        FrameEdit::Remove(frame_index) => {
            frames.remove(frame_index);
            state.selected_frame.filter(|_| !frames.is_empty()).map(|selected| selected.min(frames.len() - 1))
        },
        FrameEdit::Move(frame_index, right) => {
            let other = if right { frame_index + 1 } else { frame_index - 1 };
            frames.swap(frame_index, other);
            Some(other)
        },
        FrameEdit::Add => {
            frames.push(next_frame(frames.last(), sprite_size, cells));
            Some(frames.len() - 1)
        },
    };
    select_frame(state, controller, index, selected);
}

/// Select a frame of an animation, and center the camera on its sprite in the workspace
fn select_frame(state: &mut TimelineState, controller: &mut Controller, index: usize, frame_index: Option<usize>) {
    state.selected_frame = frame_index;
    if let Some(frame) = frame_index.and_then(|frame_index| controller.animations()[index].frames.get(frame_index).copied()) {
        let (min, size) = controller.frame_rect(&frame);
        controller.handle_action(Action::CameraCenter(min + size * 0.5));
    }
}

/// Which cell the selected frame shows, and how long it is held
fn selected_frame_controls(ui: &mut egui::Ui, state: &mut TimelineState, controller: &mut Controller, index: usize) {
    let Some(frame_index) = state.selected_frame else { return; };
    let (sprite_size, (columns, rows)) = (controller.sprite_size(), controller.sheet_cells());
    let frame = &mut controller.animations_mut()[index].frames[frame_index];

    ui.horizontal(|ui| {
        ui.label(format!("Frame {}", frame_index + 1));
        ui.separator();

        // The cell is picked on the sprite grid, so frames always line up with it
        let (mut column, mut row) = cell_of(frame.origin, sprite_size);
        ui.label("Column");
        let column_changed = ui.add(egui::DragValue::new(&mut column).range(0..=columns - 1)).changed();
        ui.label("Row");
        let row_changed = ui.add(egui::DragValue::new(&mut row).range(0..=rows - 1)).changed();
        if column_changed || row_changed {
            frame.origin = origin_of((column, row), sprite_size);
        }

        ui.label("Hold");
        ui.add(egui::DragValue::new(&mut frame.hold).range(1..=MAX_HOLD));

        // Picking the sprite by clicking it in the workspace
        ui.separator();
        let pick = ui.add(egui::Button::selectable(state.picking, "Pick in Workspace"))
            .on_hover_text("Click a sprite in the workspace to show it in this frame (I). Escape cancels.");
        if pick.clicked() {
            state.picking = !state.picking;
        }
    });
}

/// A frame of the sprite after another one in reading order, or of the first sprite if there is no other one
fn next_frame(last: Option<&Frame>, sprite_size: (u32, u32), cells: (u32, u32)) -> Frame {
    let cell = last.map_or((0, 0), |last| next_cell(cell_of(last.origin, sprite_size), cells));
    Frame { origin: origin_of(cell, sprite_size), hold: 1 }
}

/// A name for a new animation that no other animation has
fn unique_name(names: &[String]) -> String {
    (1..).map(|number| format!("Animation {number}")).find(|name| !names.contains(name)).unwrap()
}

/// Keep the light's position near the sprite, so its thumb can't be lost outside the preview
fn clamp_light_offset(offset: Vec2, size: Vec2) -> Vec2 {
    let margin = size * LIGHT_MARGIN;
    Vec2::new(offset.x.clamp(-margin.x, size.x + margin.x), offset.y.clamp(-margin.y, size.y + margin.y))
}

/// How the preview is zoomed, relative to fitting the sprite
fn zoom_label(zoom: i32) -> String {
    format!("{}%", (100.0 * 2_f32.powi(zoom)).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// New animations get the first unused number
    #[test]
    fn unique_names() {
        assert_eq!(unique_name(&[]), "Animation 1");
        assert_eq!(unique_name(&["Animation 1".into(), "Animation 3".into()]), "Animation 2");
    }

    /// Adding a frame takes the sprite after the last one, wrapping rows, or the first sprite
    #[test]
    fn next_frames() {
        let (size, cells) = ((16, 32), (2, 2));
        assert_eq!(next_frame(None, size, cells).origin, (0, 0));
        assert_eq!(next_frame(Some(&Frame { origin: (0, 0), hold: 3 }), size, cells), Frame { origin: (16, 0), hold: 1 });
        assert_eq!(next_frame(Some(&Frame { origin: (16, 0), hold: 1 }), size, cells).origin, (0, 32));
        assert_eq!(next_frame(Some(&Frame { origin: (16, 32), hold: 1 }), size, cells).origin, (0, 0));
    }

    /// The light can go a little outside the sprite, but no further
    #[test]
    fn light_clamped() {
        let size = Vec2::new(40.0, 20.0);
        assert_eq!(clamp_light_offset(Vec2::new(10.0, 10.0), size), Vec2::new(10.0, 10.0));
        assert_eq!(clamp_light_offset(Vec2::new(-50.0, 100.0), size), Vec2::new(-10.0, 25.0));
    }

    /// Positions find the whole sprite they are in, and nothing outside them
    #[test]
    fn cells_at_positions() {
        let (size, cells) = ((16, 32), (3, 2));
        assert_eq!(cell_at(Vec2::new(0.0, 0.0), size, cells), Some((0, 0)));
        assert_eq!(cell_at(Vec2::new(47.9, 63.9), size, cells), Some((2, 1)));
        assert_eq!(cell_at(Vec2::new(48.0, 10.0), size, cells), None);
        assert_eq!(cell_at(Vec2::new(-0.5, 10.0), size, cells), None);
        assert_eq!(cell_at(Vec2::new(5.0, 5.0), (0, 0), cells), None);
    }

    /// Zoom is shown as a percentage of fitting the sprite
    #[test]
    fn zoom_labels() {
        assert_eq!(zoom_label(0), "100%");
        assert_eq!(zoom_label(2), "400%");
        assert_eq!(zoom_label(-1), "50%");
    }
}
