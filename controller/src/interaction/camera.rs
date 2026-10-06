use studio_math::Vec2;

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

    pub fn scale(&self) -> f32 { 2_f32.powf(self.scale as f32) }
    pub fn inv_scale(&self) -> f32 { 2_f32.powf(-self.scale as f32) }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A default camera is at the origin with 1:1 scale
    #[test]
    fn default() {
        let camera = Camera::default();
        assert_eq!(camera.position, Vec2::ZERO);
        assert_eq!(camera.scale(), 1.0);
        assert_eq!(camera.inv_scale(), 1.0);
    }

    /// Scale is geometric, and inverse scale is its reciprocal
    #[test]
    fn scale_is_geometric() {
        let mut camera = Camera::default();
        camera.apply_scale(3, None);
        assert_eq!(camera.scale(), 8.0);
        assert_eq!(camera.inv_scale(), 0.125);

        camera.apply_scale(-5, None);
        assert_eq!(camera.scale(), 0.25);
        assert_eq!(camera.inv_scale(), 4.0);
    }

    /// Scale stays between 0.25x and 32x
    #[test]
    fn scale_is_clamped() {
        let mut camera = Camera::default();
        camera.apply_scale(100, None);
        assert_eq!(camera.scale(), 32.0);
        camera.apply_scale(-100, None);
        assert_eq!(camera.scale(), 0.25);
    }

    /// The center of the viewport is the camera position, and screen pixels are scaled
    #[test]
    fn screen_to_world() {
        let mut camera = Camera::default();
        camera.reset_position(Vec2::new(50.0, 50.0));
        let viewport = Vec2::new(100.0, 80.0);
        assert_eq!(camera.screen_to_world(Vec2::new(50.0, 40.0), viewport), Vec2::new(50.0, 50.0));
        assert_eq!(camera.screen_to_world(Vec2::ZERO, viewport), Vec2::new(0.0, 10.0));

        // At 2x each world unit takes up 2 screen pixels
        camera.apply_scale(1, None);
        assert_eq!(camera.screen_to_world(Vec2::ZERO, viewport), Vec2::new(25.0, 30.0));
    }

    /// Resetting should move the camera and reset the scale
    #[test]
    fn reset_position() {
        let mut camera = Camera::default();
        camera.apply_scale(2, None);
        camera.reset_position(Vec2::new(3.0, 4.0));
        assert_eq!(camera.position, Vec2::new(3.0, 4.0));
        assert_eq!(camera.scale(), 1.0);
    }

    /// Moving adds a delta
    #[test]
    fn move_position() {
        let mut camera = Camera::default();
        camera.move_position(Vec2::new(1.0, 2.0));
        camera.move_position(Vec2::new(1.0, 2.0));
        assert_eq!(camera.position, Vec2::new(2.0, 4.0));
    }

    /// Scaling around the camera position doesn't move the camera
    #[test]
    fn apply_scale_default_anchor() {
        let mut camera = Camera::default();
        camera.reset_position(Vec2::new(30.0, 40.0));
        camera.apply_scale(2, None);
        assert_eq!(camera.position, Vec2::new(30.0, 40.0));
    }

    /// The anchor should stay in the same place on the screen after scaling
    #[test]
    fn apply_scale_keeps_anchor_fixed() {
        let mut camera = Camera::default();
        let anchor = Vec2::new(100.0, 100.0);
        let viewport = Vec2::new(200.0, 200.0);
        let screen_before = (anchor - camera.position) * camera.scale() + viewport * 0.5;

        camera.apply_scale(1, Some(anchor));
        assert_eq!(camera.position, Vec2::new(50.0, 50.0));
        let screen_after = (anchor - camera.position) * camera.scale() + viewport * 0.5;
        assert_eq!(screen_before, screen_after);

        // Scaling past the limit shouldn't move anything
        camera.apply_scale(100, Some(anchor));
        let position = camera.position;
        camera.apply_scale(1, Some(anchor));
        assert_eq!(camera.position, position);
    }
}
