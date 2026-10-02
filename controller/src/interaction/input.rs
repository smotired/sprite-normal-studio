use vector::Vec2;

use crate::Controller;

pub enum Axis {
    Vertical,
    Horizontal,
}

/// Defines input types
pub enum Input {
    /// No input, used so that we don't kill the iterator early
    NoInput,

    /// Recentering camera to image center and scale 1
    Recenter,

    /// Moving the camera in a direction by a float amount
    CameraMove(Axis, f32),

    /// Changing the camera scale.
    /// Optionally, scale relative to a fixed anchor point in world space.
    CameraScale(i32, Option<Vec2>),

    /// Left-clicking the mouse at a world space position
    MouseClicked(Vec2),

    /// Starting a drag event
    MouseDragStarted(Vec2),

    /// Dragging the mouse while left clicking, from a start position to an end position.
    MouseDragged(Vec2, Vec2),

    /// Releasing the mouse dragging
    MouseDragReleased,

    /// Dragging the camera across this world space delta.
    CameraDragged(Vec2),

    /// Altitude is changed (i.e. page up/down is pressed). True if going up.
    Altitude(bool),

    /// Cancel the current action (esc)
    Cancel,

    /// Delete key is pressed
    Delete,

    /// Overlay is toggled on or off. Maybe later I will add an enum for OverlayComponentKind which is passed here.
    OverlayToggled,

    /// Lighting/shading is toggled.
    /// If none, toggle both. If true, toggling lighting, otherwise toggling normal.
    LightingToggled(Option<bool>),
}

impl Controller {
    /// Set the cursor position in world space.
    pub fn set_cursor_pos(&mut self, pos: Vec2) {
        self.cursor_pos = pos;
    }

    // Handle different inputs from the UI
    pub fn handle_input(&mut self, input: Input) {
        match input {
            Input::Recenter => {
                let center = Vec2::from(self.output().size()) * 0.5;
                self.camera.reset_position(center);
                self.light.set_pos(center);
            },

            Input::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.move_position(Vec2::vt(amount * self.camera.inv_scale()));
                    },
                    Axis::Horizontal => {
                        self.camera.move_position(Vec2::hz(amount * self.camera.inv_scale()));
                    },
                }
            },

            Input::CameraScale(amount, relative_to) => {
                self.camera.apply_scale(amount, relative_to);
            },

            Input::MouseClicked(pos) => {
                if self.overlay_state.overlay_on() {
                    let result = self.tool.handle_click(self.create_state(), pos);
                    self.handle_tool_result(result);
                }
            },

            Input::MouseDragStarted(pos) => {
                self.overlay_state.dragging_light = false;

                if self.overlay_state.overlay_on() {
                    // Decide if we should start dragging the light, which takes priority over the tool
                    if self.light.distance(pos) * self.camera.scale() <= 10.0 {
                        self.overlay_state.dragging_light = true;
                    }

                    // Check the tool
                    else {
                        let result = self.tool.handle_drag_start(self.create_state(), pos);
                        self.handle_tool_result(result);
                    }
                }
            }

            Input::MouseDragged(_start, new) => {
                // Move the light if we are dragging it
                if self.overlay_state.dragging_light {
                    self.light.set_pos(new);
                }

                // Otherwise switch based on tool
                else if self.overlay_state.overlay_on() {
                    let result = self.tool.handle_dragging_to(self.create_state(), new);
                    self.handle_tool_result(result);
                }
            },

            Input::MouseDragReleased => {
                if self.overlay_state.dragging_light {
                    self.overlay_state.dragging_light = false;
                }

                else if self.overlay_state.overlay_on() {
                    let result = self.tool.handle_drag_released(self.create_state());
                    self.handle_tool_result(result);
                }
            },

            Input::Cancel => {
                let result = self.tool.handle_cancel(self.create_state());
                self.handle_tool_result(result);
            },

            Input::Delete => {
                let result = self.tool.handle_delete(self.create_state());
                self.handle_tool_result(result);
            },

            Input::Altitude(up) => {
                if self.overlay_state.dragging_light {
                    self.light.adjust_height(if up { 100.0 } else { -100.0 });
                }

                // TODO: maybe move a zone between layers in Zone tool?
                // Layers are in like phase 6 or something though
            },

            Input::OverlayToggled => {
                self.overlay_state.toggle_overlay();
            },

            Input::CameraDragged(delta) => {
                self.camera.move_position(-delta);
            },

            Input::LightingToggled(toggle) => {
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

            Input::NoInput => { },
        }
    }
}