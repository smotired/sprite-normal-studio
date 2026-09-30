#[derive(Copy, Clone)]
pub struct Light {
    pos: (f32, f32, f32),
    color: (u8, u8, u8),
}

impl Light {
    pub fn new() -> Self { Default::default() }

    pub fn set_pos(&mut self, pos: (f32, f32)) {
        self.pos = (pos.0, pos.1, self.pos.2);
    }

    pub fn adjust_height(&mut self, delta: f32) {
        self.pos.2 = (self.pos.2 + delta).max(100.0);
    }

    pub fn distance(&self, pos: (f32, f32)) -> f32 {
        let dx = self.pos.0 - pos.0;
        let dy = self.pos.1 - pos.1;
        (dx * dx + dy * dy).sqrt()
    }

    // Get position as an array and color as a packed int
    pub fn info_for_shader(&self) -> ([f32; 3], u32) {
        (
            [ self.pos.0, self.pos.1, self.pos.2 ],
            (self.color.0 as u32) | (self.color.1 as u32) << 8 | (self.color.2 as u32) << 16,
        )
    }
}

impl Default for Light {
    fn default() -> Self {
        Self {
            pos: (100.0, 100.0, 200.0),
            color: (255, 255, 255),
        }
    }
}