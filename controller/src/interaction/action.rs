use studio_math::Vec2;

use crate::{Controller, EditorToolKind, PastePlacement, SpecialAction};
use super::history::DragStep;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Axis {
    Vertical,
    Horizontal,
}

#[derive(Default, Copy, Clone, Debug)]
pub struct InputModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl InputModifiers {
    pub fn none()       -> Self { Default::default() }
    pub fn shift()      -> Self { Self { shift: true,  ctrl: false, alt: false }}
    pub fn ctrl()       -> Self { Self { shift: false, ctrl: true,  alt: false }}
    pub fn alt()        -> Self { Self { shift: false, ctrl: false, alt: true  }}
    pub fn shift_ctrl() -> Self { Self { shift: true,  ctrl: true,  alt: false }}
    pub fn ctrl_shift() -> Self { Self { shift: true,  ctrl: true,  alt: false }}
    pub fn shift_alt()  -> Self { Self { shift: true,  ctrl: false, alt: true  }}
    pub fn alt_shift()  -> Self { Self { shift: true,  ctrl: false, alt: true  }}
    pub fn ctrl_alt()   -> Self { Self { shift: false, ctrl: true,  alt: true  }}
    pub fn alt_ctrl()   -> Self { Self { shift: false, ctrl: true,  alt: true  }}
    pub fn all()        -> Self { Self { shift: true,  ctrl: true,  alt: true  }}
}

/// Defines actions that can be performed on the controller, from raw input or from the UI
pub enum Action {
    /// No action, used so that we don't kill the iterator early
    None,

    /// Recentering camera to image center and scale 1
    Recenter,

    /// Moving the camera in a direction by a float amount
    CameraMove(Axis, f32),

    /// Changing the camera scale.
    /// Optionally, scale relative to a fixed anchor point in world space.
    CameraScale(i32, Option<Vec2>),

    /// Left-clicking the mouse at a world space position
    MouseClicked(Vec2, InputModifiers),

    /// Starting a drag event
    MouseDragStarted(Vec2, InputModifiers),

    /// Dragging the mouse while left clicking, from a start position to an end position.
    MouseDragged(Vec2, Vec2, InputModifiers),

    /// Releasing the mouse dragging
    MouseDragReleased(InputModifiers),

    /// Dragging the camera across this world space delta.
    CameraDragged(Vec2),

    /// Altitude is changed (i.e. page up/down is pressed). True if going up.
    Altitude(bool),

    /// Cancel the current action (esc)
    Cancel,

    /// Delete key is pressed
    Delete,

    /// Select everything the selected tool can select
    SelectAll,

    /// Copy the selected zones and paste them, without using the clipboard
    Duplicate(PastePlacement),

    /// Undo the latest action or drag
    Undo,

    /// Redo the latest undone action or drag
    Redo,

    /// Overlay is toggled on or off. Maybe later I will add an enum for OverlayComponentKind which is passed here.
    OverlayToggled,

    /// Zone paths in the overlay are toggled on or off, which also hides shapes
    PathsToggled,

    /// Shapes in the overlay are toggled on or off. The shape tools always show them.
    ShapesToggled,

    /// A specific editor tool is selected.
    ToolSelected(EditorToolKind),

    /// An action from the UI that the selected tool handles in its own way.
    Special(SpecialAction),

    /// Lighting/shading is toggled.
    /// If none, toggle both. If true, toggling lighting, otherwise toggling normal.
    LightingToggled(Option<bool>),
}

impl Controller {
    /// Set the cursor position in world space.
    pub fn set_cursor_pos(&mut self, pos: Vec2) {
        self.cursor_pos = pos;
    }

    /// If the sprite is lit by the light
    pub fn lighting_on(&self) -> bool { self.overlay_state.lighting_on() }

    /// If the sprite is shaded with the normal map
    pub fn normals_on(&self) -> bool { self.overlay_state.normals_on() }

    /// If the overlay is shown
    pub fn overlay_on(&self) -> bool { self.overlay_state.overlay_on() }

    /// If zone paths are shown in the overlay
    pub fn paths_on(&self) -> bool { self.overlay_state.paths_on() }

    /// If shapes are shown in the overlay outside the shape tools
    pub fn shapes_on(&self) -> bool { self.overlay_state.shapes_on() }

    // Handle different actions from the UI
    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Recenter => {
                let center = Vec2::from(self.output().size()) * 0.5;
                self.camera.reset_position(center);
                self.light.set_pos(center);
            },

            Action::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.move_position(Vec2::vt(amount * self.camera.inv_scale()));
                    },
                    Axis::Horizontal => {
                        self.camera.move_position(Vec2::hz(amount * self.camera.inv_scale()));
                    },
                }
            },

            Action::CameraScale(amount, relative_to) => {
                self.camera.apply_scale(amount, relative_to);
            },

            Action::MouseClicked(pos, modifiers) => {
                if self.overlay_state.overlay_on() {
                    self.run_tool(DragStep::None, |tool, state| tool.handle_click(state, pos, modifiers));
                }
            },

            Action::MouseDragStarted(pos, modifiers) => {
                self.overlay_state.dragging_light = false;

                if self.overlay_state.overlay_on() {
                    // Decide if we should start dragging the light, which takes priority over the tool
                    if self.light.distance(pos) * self.camera.scale() <= 10.0 {
                        self.overlay_state.dragging_light = true;
                    }

                    // Check the tool
                    else {
                        self.run_tool(DragStep::None, |tool, state| tool.handle_drag_start(state, pos, modifiers));
                    }
                }
            }

            Action::MouseDragged(_start, new, modifiers) => {
                // Move the light if we are dragging it
                if self.overlay_state.dragging_light {
                    self.light.set_pos(new);
                }

                // Otherwise switch based on tool
                else if self.overlay_state.overlay_on() {
                    self.run_tool(DragStep::Continue, |tool, state| tool.handle_dragging_to(state, new, modifiers));
                }
            },

            Action::MouseDragReleased(modifiers) => {
                if self.overlay_state.dragging_light {
                    self.overlay_state.dragging_light = false;
                }

                else if self.overlay_state.overlay_on() {
                    self.run_tool(DragStep::Release, |tool, state| tool.handle_drag_released(state, modifiers));
                }
            },

            Action::Cancel => {
                self.run_tool(DragStep::None, |tool, state| tool.handle_cancel(state));
            },

            Action::Delete => {
                self.run_tool(DragStep::None, |tool, state| tool.handle_delete(state));
            },

            Action::SelectAll => {
                self.run_tool(DragStep::None, |tool, state| tool.handle_select_all(state));
            },

            Action::Duplicate(placement) => {
                if let Some(objects) = self.copy_selection() {
                    self.paste(&objects, placement);
                }
            },

            Action::Undo => {
                self.undo();
            },

            Action::Redo => {
                self.redo();
            },

            Action::Altitude(up) => {
                if self.overlay_state.dragging_light {
                    self.light.adjust_height(if up { 100.0 } else { -100.0 });
                }

                // TODO: maybe move a zone between layers in Zone tool?
                // Layers are in like phase 6 or something though
            },

            Action::OverlayToggled => {
                self.overlay_state.toggle_overlay();
            },

            Action::PathsToggled => {
                self.overlay_state.toggle_paths();
            },

            Action::ShapesToggled => {
                self.overlay_state.toggle_shapes();
            },

            Action::ToolSelected(tool) => {
                self.select_tool(tool);
            },

            Action::CameraDragged(delta) => {
                self.camera.move_position(-delta);
            },

            Action::LightingToggled(toggle) => {
                if let Some(toggling_lighting) = toggle {
                    if toggling_lighting {
                        self.overlay_state.lighting_on = !self.overlay_state.lighting_on;
                    } else {
                        self.overlay_state.normals_on = !self.overlay_state.normals_on;
                    }
                } else {
                    // Go to fully shaded unless we are already at fully shaded
                    if self.overlay_state.lighting_on && self.overlay_state.normals_on {
                        self.overlay_state.lighting_on = false;
                        self.overlay_state.normals_on = false;
                    } else {
                        self.overlay_state.lighting_on = true;
                        self.overlay_state.normals_on = true;
                    }
                }
            },

            Action::Special(action) => {
                self.run_special_action(action);
            },

            Action::None => { },
        }
    }
}