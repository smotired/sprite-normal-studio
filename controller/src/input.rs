use crate::{Controller, Input::NoInput, camera::Camera};

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
    CameraScale(f32, Option<(f32, f32)>),
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

            NoInput => { },
        }
    }
}