use studio_math::Vec2;

#[derive(Copy, Clone)]
pub struct Light {
    position: Vec2,
    height: f32,
    color: (u8, u8, u8),
}

impl Light {
    pub fn set_pos(&mut self, pos: Vec2) { self.position = pos; }

    pub fn adjust_height(&mut self, delta: f32) {
        self.height = (self.height + delta).max(100.0);
    }

    pub fn distance(&self, pos: Vec2) -> f32 { self.position.distance(pos) }

    /// Set every property of the light at once. Height is kept at 100 or above, as when adjusting it.
    pub fn set(&mut self, position: Vec2, height: f32, color: (u8, u8, u8)) {
        self.position = position;
        self.height = height.max(100.0);
        self.color = color;
    }

    pub fn position(&self) -> Vec2 { self.position }
    pub fn height(&self) -> f32 { self.height }
    pub fn color(&self) -> (u8, u8, u8) { self.color }
    pub fn packed_color(&self) -> u32 { (self.color.0 as u32) | (self.color.1 as u32) << 8 | (self.color.2 as u32) << 16 }
}

impl Default for Light {
    fn default() -> Self {
        Self {
            position: Vec2::new(100.0, 100.0),
            height: 200.0,
            color: (255, 255, 255),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Check defaults
    #[test]
    fn default() {
        let light = Light::default();
        assert_eq!(light.position(), Vec2::new(100.0, 100.0));
        assert_eq!(light.height(), 200.0);
    }

    /// Position and distance
    #[test]
    fn position_and_distance() {
        let mut light = Light::default();
        light.set_pos(Vec2::new(3.0, 4.0));
        assert_eq!(light.position(), Vec2::new(3.0, 4.0));
        assert_eq!(light.distance(Vec2::ZERO), 5.0);
    }

    /// Height can go up and down but not below 100
    #[test]
    fn adjust_height() {
        let mut light = Light::default();
        light.adjust_height(100.0);
        assert_eq!(light.height(), 300.0);
        light.adjust_height(-150.0);
        assert_eq!(light.height(), 150.0);
        light.adjust_height(-1000.0);
        assert_eq!(light.height(), 100.0);
    }

    /// Setting the light replaces everything, but still keeps the height at 100 or above
    #[test]
    fn set() {
        let mut light = Light::default();
        light.set(Vec2::new(1.0, 2.0), 150.0, (1, 2, 3));
        assert_eq!((light.position(), light.height(), light.color()), (Vec2::new(1.0, 2.0), 150.0, (1, 2, 3)));
        light.set(Vec2::ZERO, 10.0, (0, 0, 0));
        assert_eq!(light.height(), 100.0);
    }

    /// Color is packed with red in the lowest byte
    #[test]
    fn packed_color() {
        assert_eq!(Light::default().packed_color(), 0x00FFFFFF);

        let light = Light { color: (0x11, 0x22, 0x33), ..Light::default() };
        assert_eq!(light.packed_color(), 0x00332211);
    }
}
