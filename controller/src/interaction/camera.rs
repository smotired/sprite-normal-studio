use vector::Vec2;

/// Defines the current state of the camera.
#[derive(Copy, Clone, Default)]
pub struct Camera {
    // Position of the pixel the camera is centered on, from the top left of the image, if scale = 1
    pub position: Vec2,

    // Scale of the camera. Determines how big the pixels are. Starts at 0 for 1:1 scale.
    // pixel_size = 2 ^ scale, so scaling is done geometrically by 2.
    scale: i32,
}

impl Camera {
    // Convert viewport pixel position to world/spritesheet space.
    pub fn screen_to_world(&self, screen: Vec2, viewport_size: Vec2) -> Vec2 {
        (screen - viewport_size * 0.5) * self.inv_scale() + self.position
    }

    // Move the camera to a specific position and reset scale
    pub fn reset_position(&mut self, pos: Vec2) { self.position = pos; self.scale = 0; }

    // Move the camera by a position delta
    pub fn move_position(&mut self, delta: Vec2) { self.position += delta; }

    // Update the camera scale, keeping relative_to fixed on screen. Defaults to current camera position.
    pub fn apply_scale(&mut self, factor: i32, relative_to: Option<Vec2>) {
        let anchor = relative_to.unwrap_or(self.position);
        let old_real_scale = self.scale();
        self.scale = (self.scale + factor).clamp(-2, 5); // kind of arbitrary bounds for 0.25x to 32x
        let ratio = old_real_scale / self.scale();

        // Update position to keep the anchor in the same place
        self.position = anchor + (self.position - anchor) * ratio; // lerp clamps so do manually
    }

    pub fn scale(&self) -> f32 { (2 as f32).powf(self.scale as f32) }
    pub fn inv_scale(&self) -> f32 { (2 as f32).powf(-self.scale as f32) }
}