use crate::{Controller, camera::Camera};

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

    /// Changing the camera scale. Should be a power of 2.
    /// Optionally, scale relative to a fixed anchor point in world space.
    CameraScale(f32, Option<(f32, f32)>),

    /// Left-clicking the mouse at a world space position
    MouseClicked((f32, f32)),

    /// Starting a drag event
    MouseDragStarted((f32, f32)),

    /// Dragging the mouse while left clicking, from a start position to an end position.
    MouseDragged((f32, f32), (f32, f32)),

    /// Releasing the mouse dragging
    MouseDragReleased,
}

impl Controller {
    // Handle different inputs from the controller
    pub fn handle_input(&mut self, input: Input) {
        match input {
            Input::Recenter => {
                let size = self.output().size();
                self.camera = Camera::new([size.width as f32 * 0.5, size.height as f32 * 0.5], 1);
            },

            Input::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.move_position((0.0, amount / self.camera.scale as f32));
                    },
                    Axis::Horizontal => {
                        self.camera.move_position((amount / self.camera.scale as f32, 0.0));
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

                // Decide if we should start dragging the light
                if self.light.distance((x, y)) * self.camera.scale as f32 <= 10.0 {
                    self.overlay_state.dragging_light = true;
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

            Input::NoInput => { },
        }
    }
}