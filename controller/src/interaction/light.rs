use vector::V2;

#[derive(Copy, Clone)]
pub struct Light {
    position: V2,
    height: f32,
    color: (u8, u8, u8),
}

impl Light {
    pub fn set_pos(&mut self, pos: V2) { self.position = pos; }

    pub fn adjust_height(&mut self, delta: f32) {
        self.height = (self.height + delta).max(100.0);
    }

    pub fn distance(&self, pos: V2) -> f32 { self.position.distance(pos) }

    pub fn pos_arr(&self) -> [f32; 3] { [ self.position.x, self.position.y, self.height ] }
    pub fn packed_color(&self) -> u32 { (self.color.0 as u32) | (self.color.1 as u32) << 8 | (self.color.2 as u32) << 16 }
}

impl Default for Light {
    fn default() -> Self {
        Self {
            position: V2::new(100.0, 100.0),
            height: 200.0,
            color: (255, 255, 255),
        }
    }
}