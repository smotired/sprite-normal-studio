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
    CameraScale(f32),
}

impl Controller {
    // Handle different inputs from the controller
    pub fn handle_input(&mut self, input: Input) {
        match input {
            Input::Recenter => {
                let size = self.output().size();
                self.camera = Camera::new([(size.width / 2) as i32, (size.height / 2) as i32], 1);
            },

            Input::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.set_pos([self.camera.position[0], self.camera.position[1] + amount as i32]);
                    },
                    Axis::Horizontal => {
                        self.camera.set_pos([self.camera.position[0] + amount as i32, self.camera.position[1]]);
                    },
                }
            },

            Input::CameraScale(amount) => {
                self.camera.set_scale((self.camera.scale as f32 * amount) as u32);
            },

            NoInput => { },
        }
    }
}