/// Defines the current state of the camera.
#[derive(Copy, Clone)]
pub struct Camera {
    // Position of the pixel the camera is centered on, from the top left of the image, if scale = 1
    pub position: [f32; 2],

    // Scale of the camera. At scale 2, each screen pixel is 2 pixels of the image. Always an integer >= 1.
    pub scale: u32,
}

impl Camera {
    pub fn new(position: [f32; 2], scale: u32) -> Self {
        Self { position, scale }
    }

    // Convert viewport pixel position to world/spritesheet space.
    pub fn screen_to_world(&self, screen: (f32, f32), viewport_size: (f32, f32)) -> (f32, f32) {
        let half = (viewport_size.0 / 2.0, viewport_size.1 / 2.0);
        (
            (screen.0 - half.0) / self.scale as f32 + self.position[0],
            (screen.1 - half.1) / self.scale as f32 + self.position[1]
        )
    }

    // Move the camera by a position delta
    pub fn move_position(&mut self, delta: (f32, f32)) {
        self.position[0] += delta.0;
        self.position[1] += delta.1;
    }

    // Update the camera scale, keeping relative_to fixed on screen. Defaults to current camera position.
    // factor should be a power of 2.
    pub fn apply_scale(&mut self, factor: f32, relative_to: Option<(f32, f32)>) {
        let anchor = relative_to.unwrap_or((self.position[0], self.position[1]));
        let new_scale = ((self.scale as f32 * factor) as u32).clamp(1, 64);
        let ratio = self.scale as f32 / new_scale as f32;

        self.position[0] = anchor.0 + (self.position[0] - anchor.0) * ratio;
        self.position[1] = anchor.1 + (self.position[1] - anchor.1) * ratio;
        self.scale = new_scale;
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            scale: 1,
        }
    }
}