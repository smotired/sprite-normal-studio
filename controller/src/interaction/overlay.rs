use crate::{EditorToolKind, SpecialQuery};
use crate::interaction::tools::EditorTool;

/// Tracks the state of the overlay
pub struct OverlayState {
    pub dragging_light: bool,
    pub lighting_on: bool,
    pub normals_on: bool,
    overlay_on: bool,

    /// If zone paths are drawn. Shapes are only drawn along with them.
    paths_on: bool,

    /// If shapes are drawn outside the shape tools, which always draw them
    shapes_on: bool,
}

impl OverlayState {
    // Packs flags into a uint
    pub fn get_flags(&self, tool: &dyn EditorTool) -> u32 {
        let creating = tool.creating_path();
        let edits_shapes = tool.handle_special_query(SpecialQuery::ActiveZone).is_some();
        let drawing_shapes = tool.handle_special_query(SpecialQuery::ShapeGhost).is_some();
        let tool = tool.kind();
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
            // 09: Shapes, which are only drawn with the paths. The shape tools always draw them.
            // 10: A shape tool is selected, so the active zone's shape vertices get knobs
            // 11: A ghost line from the latest vertex of the path of shapes being drawn to the cursor
            if self.paths_on {
                flags |= 1 << 4;
                if self.shapes_on || edits_shapes { flags |= 1 << 9; }
                if edits_shapes { flags |= 1 << 10; }
                if drawing_shapes { flags |= 1 << 11; }
            }

            // 05: If the currently selected zone path should have its points drawn
            match tool {
                EditorToolKind::Point | EditorToolKind::Pen => {
                    flags |= 1 << 5;
                },
                _ => {}
            }

            // 06: If a path is being created (it is the last zone and should not be closed)
            // 07: If the latest point of the path being created is its first point, so the ghost path closes to it
            if let Some(closing) = creating {
                flags |= 1 << 6;
                if closing { flags |= 1 << 7; }
            }
        }

        flags
    }

    pub fn overlay_on(&self) -> bool { self.overlay_on }

    pub fn lighting_on(&self) -> bool { self.lighting_on }

    pub fn normals_on(&self) -> bool { self.normals_on }

    pub fn paths_on(&self) -> bool { self.paths_on }

    pub fn shapes_on(&self) -> bool { self.shapes_on }

    pub fn toggle_paths(&mut self) { self.paths_on = !self.paths_on; }

    pub fn toggle_shapes(&mut self) { self.shapes_on = !self.shapes_on; }

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
            paths_on: true,
            shapes_on: false,
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::interaction::tools::{EditorToolPen, EditorToolPoint, EditorToolZone};

    use super::*;

    /// Everything on by default, but points aren't drawn in the zone tool
    #[test]
    fn default_flags_zone_tool() {
        let tool = EditorToolZone::init();
        assert_eq!(OverlayState::default().get_flags(tool.as_ref()), 0b0001_0111);
    }

    /// Point and pen tools also draw the points
    #[test]
    fn default_flags_point_tools() {
        let state = OverlayState::default();
        assert_eq!(state.get_flags(EditorToolPoint::init().as_ref()), 0b0011_0111);
        assert_eq!(state.get_flags(EditorToolPen::init().as_ref()), 0b0011_0111);
    }

    /// With the overlay off, only shading flags are set
    #[test]
    fn overlay_off_flags() {
        let mut state = OverlayState::default();
        state.toggle_overlay();
        assert_eq!(state.get_flags(EditorToolPen::init().as_ref()), 0b11);

        state.normals_on = false;
        assert_eq!(state.get_flags(EditorToolPen::init().as_ref()), 0b10);
    }

    /// Without lighting, the light isn't drawn unless it's being dragged
    #[test]
    fn light_flags() {
        let tool = EditorToolZone::init();
        let mut state = OverlayState { lighting_on: false, normals_on: false, ..Default::default() };
        assert_eq!(state.get_flags(tool.as_ref()), 0b1_0000);

        state.dragging_light = true;
        assert_eq!(state.get_flags(tool.as_ref()), 0b1_1100);
    }

    /// Shapes are off by default, and only drawn along with the paths
    #[test]
    fn path_and_shape_flags() {
        let tool = EditorToolZone::init();
        let mut state = OverlayState::default();
        assert!(state.paths_on() && !state.shapes_on());

        state.toggle_shapes();
        assert_eq!(state.get_flags(tool.as_ref()), 0b10_0001_0111);

        state.toggle_paths();
        assert_eq!(state.get_flags(tool.as_ref()), 0b0111);

        state.toggle_overlay();
        state.toggle_paths();
        assert_eq!(state.get_flags(tool.as_ref()), 0b11);
    }

    /// Toggling reports the new state, and turning the overlay off stops dragging the light
    #[test]
    fn toggle_overlay() {
        let mut state = OverlayState::default();
        assert!(state.overlay_on());

        state.dragging_light = true;
        assert!(!state.toggle_overlay());
        assert!(!state.overlay_on());
        assert!(!state.dragging_light);

        assert!(state.toggle_overlay());
        assert!(state.overlay_on());
    }
}
