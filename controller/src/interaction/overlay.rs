/// Tracks the state of the overlay
pub struct OverlayState {
    pub dragging_light: bool,
    pub lighting_on: bool,
    pub normals_on: bool,
    overlay_on: bool,

}

impl OverlayState {
    // Packs flags into a uint
    pub fn get_flags(&self) -> u32 {
        let mut flags: u32 = 0;

        // 00: Shade with normal map
        if self.normals_on { flags |= 1 << 0; }

        // 01: Have lighting on sprite. If disabled and normals are enabled, draws normal map instead of sprite.
        if self.lighting_on { flags |= 1 << 1; }

        // The following features are only set if the whole overlay is on
        if self.overlay_on {
            // Only render anything with the lights if lighting is on (or we're still dragging it. might be useful)
            if self.lighting_on || self.dragging_light {
                // 02: Light button
                flags |= 1 << 2;

                // 03: Light halo
                if self.dragging_light { flags |= 1 << 3; }
            }

            // 04: Overlay paths
            flags |= 1 << 4;
        }

        flags
    }

    pub fn overlay_on(&self) -> bool { self.overlay_on }

    pub fn toggle_overlay(&mut self) -> bool {
        if self.overlay_on {
            self.dragging_light = false;
            self.overlay_on = false;
        } else {
            self.overlay_on = true;
        }

        self.overlay_on
    }
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            lighting_on: true,
            normals_on: true,
            dragging_light: false,
            overlay_on: true,
        }
    }
}