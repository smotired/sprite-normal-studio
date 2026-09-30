use crate::interaction::Interaction;

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
    CameraScale(i32, Option<(f32, f32)>),

    /// Left-clicking the mouse at a world space position
    MouseClicked((f32, f32)),

    /// Starting a drag event
    MouseDragStarted((f32, f32)),

    /// Dragging the mouse while left clicking, from a start position to an end position.
    MouseDragged((f32, f32), (f32, f32)),

    /// Releasing the mouse dragging
    MouseDragReleased,

    /// Dragging the camera across this world space delta.
    CameraDragged((f32, f32)),

    /// Altitude is changed (i.e. page up/down is pressed). True if going up.
    Altitude(bool),

    /// Overlay is toggled on or off. Maybe later I will add an enum for OverlayComponentKind which is passed here.
    OverlayToggled,

    /// Lighting/shading is toggled.
    /// If none, toggle both. If true, toggling lighting, otherwise toggling normal.
    LightingToggled(Option<bool>),
}

impl Interaction {
    // Handle different inputs from the UI
    pub fn handle_input(&mut self, input: Input, viewport_size: (f32, f32)) {
        match input {
            Input::Recenter => {
                self.camera.set_position((viewport_size.0 * 0.5, viewport_size.1 as f32 * 0.5));
                self.light.set_pos((viewport_size.0 as f32 * 0.5, viewport_size.1 as f32 * 0.5));
            },

            Input::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.move_position((0.0, amount * self.camera.inv_scale()));
                    },
                    Axis::Horizontal => {
                        self.camera.move_position((amount * self.camera.inv_scale(), 0.0));
                    },
                }
            },

            Input::CameraScale(amount, relative_to) => {
                self.camera.apply_scale(amount, relative_to);
            },

            Input::MouseClicked((x, y)) => {
                println!("Clicked: ({}, {})", x, y);  
            },

            Input::MouseDragStarted((x, y)) => {
                self.overlay_state.dragging_light = false;

                if self.overlay_state.overlay_on() {
                    // Decide if we should start dragging the light
                    if self.light.distance((x, y)) * self.camera.scale() <= 10.0 {
                        self.overlay_state.dragging_light = true;
                    }
                }
            }

            Input::MouseDragged((_sx, _sy), (nx, ny)) => {
                // Move the light if we are dragging it
                if self.overlay_state.dragging_light {
                    self.light.set_pos((nx, ny));
                }
            },

            Input::MouseDragReleased => {
                self.overlay_state.dragging_light = false;
            },

            Input::Altitude(up) => {
                if self.overlay_state.dragging_light {
                    self.light.adjust_height(if up { 100.0 } else { -100.0 });
                }
            },

            Input::OverlayToggled => {
                self.overlay_state.toggle_overlay();
            },

            Input::CameraDragged((dx, dy)) => {
                self.camera.move_position((-dx, -dy));
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