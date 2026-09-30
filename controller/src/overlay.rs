/// Tracks the state of the overlay
pub struct OverlayState {
    pub dragging_light: bool,
}

impl OverlayState {
    pub fn new() -> Self {
        Self {
            dragging_light: false,
        }
    }

    // Packs flags into a uint
    pub fn get_flags(&self) -> u32 {
        let mut flags: u32 = 0;

        // 00: Draw normal map instead of sprite
        // flags |= 1 << 0;

        // 01: Have lighting on sprite
        flags |= 1 << 1;

        // 02: Light button
        flags |= 1 << 2;

        // 03: Light halo
        if self.dragging_light { flags |= 1 << 3; }

        flags
    }
}