/// Defines the current state of the camera.
#[derive(Copy, Clone)]
pub struct Camera {
    // Position of the pixel the camera is centered on, from the top left of the image, if scale = 1
    pub position: [f32; 2],

    // Scale of the camera. Determines how big the pixels are. Starts at 0 for 1:1 scale.
    // pixel_size = 2 ^ scale, so scaling is done geometrically by sqrt(2).
    scale: i32,
}

impl Camera {
    // Convert viewport pixel position to world/spritesheet space.
    pub fn screen_to_world(&self, screen: (f32, f32), viewport_size: (f32, f32)) -> (f32, f32) {
        let half = (viewport_size.0 / 2.0, viewport_size.1 / 2.0);
        (
            (screen.0 - half.0) * self.inv_scale() + self.position[0],
            (screen.1 - half.1) * self.inv_scale() + self.position[1]
        )
    }

    // Move the camera by a position delta
    pub fn set_position(&mut self, pos: (f32, f32)) {
        self.position[0] = pos.0;
        self.position[1] = pos.1;
    }

    // Move the camera by a position delta
    pub fn move_position(&mut self, delta: (f32, f32)) {
        self.position[0] += delta.0;
        self.position[1] += delta.1;
    }

    // Update the camera scale, keeping relative_to fixed on screen. Defaults to current camera position.
    pub fn apply_scale(&mut self, factor: i32, relative_to: Option<(f32, f32)>) {
        let anchor = relative_to.unwrap_or((self.position[0], self.position[1]));
        let old_real_scale = self.scale();
        self.scale = (self.scale + factor).clamp(-2, 5); // kind of arbitrary bounds for 0.25x to 32x
        let ratio = old_real_scale / self.scale();

        // Update position to keep the anchor in the same place
        self.position[0] = anchor.0 + (self.position[0] - anchor.0) * ratio;
        self.position[1] = anchor.1 + (self.position[1] - anchor.1) * ratio;
    }

    pub fn scale(&self) -> f32 { (2 as f32).powf(self.scale as f32) }
    pub fn inv_scale(&self) -> f32 { (2 as f32).powf(-self.scale as f32) }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            scale: 0,
        }
    }
}