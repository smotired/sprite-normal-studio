use vector::Vec2;

use crate::{Controller, EditorTool};
use crate::objects::ControlPointMode;

pub enum Axis {
    Vertical,
    Horizontal,
}

/// Defines input types
pub enum Input {
    /// No input, used so that we don't kill the iterator early
    NoInput,

    /// Recentering camera to image center and scale 1
    Recenter,

    /// Moving the camera in a direction by a float amount
    CameraMove(Axis, f32),

    /// Changing the camera scale.
    /// Optionally, scale relative to a fixed anchor point in world space.
    CameraScale(i32, Option<Vec2>),

    /// Left-clicking the mouse at a world space position
    MouseClicked(Vec2),

    /// Starting a drag event
    MouseDragStarted(Vec2),

    /// Dragging the mouse while left clicking, from a start position to an end position.
    MouseDragged(Vec2, Vec2),

    /// Releasing the mouse dragging
    MouseDragReleased,

    /// Dragging the camera across this world space delta.
    CameraDragged(Vec2),

    /// Altitude is changed (i.e. page up/down is pressed). True if going up.
    Altitude(bool),

    /// Overlay is toggled on or off. Maybe later I will add an enum for OverlayComponentKind which is passed here.
    OverlayToggled,

    /// Lighting/shading is toggled.
    /// If none, toggle both. If true, toggling lighting, otherwise toggling normal.
    LightingToggled(Option<bool>),
}

impl Controller {
    // Get the closest control point to a click position, if it's in the click range corrected for camera scale.
    fn get_clicked_control_point(&self, zone_id: Option<u16>, pos: Vec2) -> Option<(u16, u16)> {
        if let Some((zone_id, point_id)) = self.objects.get_closest_point(zone_id, pos) {
            let distance = self.objects.get_point(point_id).unwrap().absolute_axis_distance(pos);
            if distance * self.camera.inv_scale() <= 4.0 { // size of control point boxes in the overlay, plus 1 pixel
                return Some((zone_id, point_id));
            }
        }
        None
    }

    // Get the handle that was clicked for the selected control point, if applicable.
    // Returns true if the right handle was clicked, and None if no handle was clicked.
    fn get_clicked_handle(&self, pos: Vec2) -> Option<bool> {
        if let Some(point_id) = self.selected_point {
            let point = self.objects.get_point(point_id).unwrap();
            if let ControlPointMode::Linear = point.mode() { return None; }

            // Check the left handle
            let left_handle_distance = point.left_handle().distance(pos) * self.camera.inv_scale();
            if left_handle_distance <= 4.0 {
                return Some(false); // left handle clicked
            }

            // Check the right handle
            let right_handle_distance = point.right_handle().distance(pos) * self.camera.inv_scale();
            if right_handle_distance <= 4.0 {
                return Some(true); // right handle clicked
            }
        }
        None
    }

    // Handle different inputs from the UI
    pub fn handle_input(&mut self, input: Input) {
        match input {
            Input::Recenter => {
                let center = Vec2::from(self.output().size()) * 0.5;
                self.camera.reset_position(center);
                self.light.set_pos(center);
            },

            Input::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.move_position(Vec2::vt(amount * self.camera.inv_scale()));
                    },
                    Axis::Horizontal => {
                        self.camera.move_position(Vec2::hz(amount * self.camera.inv_scale()));
                    },
                }
            },

            Input::CameraScale(amount, relative_to) => {
                self.camera.apply_scale(amount, relative_to);
            },

            Input::MouseClicked(pos) => {
                match self.tool {
                    EditorTool::Zone => {
                        // Select the zone if a control point was clicked
                        if let Some((zone_id, _)) = self.get_clicked_control_point(None, pos) {
                            self.selected_zone = Some(zone_id);
                        } else {
                            self.selected_zone = None;
                        }
                    },
                    EditorTool::Point => {
                        // If we have a control point selected, check if we clicked its handle
                        if let Some(_) = self.get_clicked_handle(pos) {
                            // Just don't deselect
                        }

                        // Select the zone and point if a control point was clicked
                        else if let Some((zone_id, point_id)) = self.get_clicked_control_point(None, pos) {
                            self.selected_zone = Some(zone_id);
                            self.selected_point = Some(point_id);
                        } else {
                            self.selected_zone = None;
                            self.selected_point = None;
                        }
                    },
                    EditorTool::Pen => {
                        // If we have a zone selected, we are already creating one, so add a linear node
                        if let Some(zone_id) = self.selected_zone {
                            
                        }
                        // Otherwise, we should try to create a zone by adding a linear node
                        else {

                        }
                    }
                }
            },

            Input::MouseDragStarted(pos) => {
                self.overlay_state.dragging_light = false;

                if self.overlay_state.overlay_on() {
                    // Decide if we should start dragging the light
                    if self.light.distance(pos) * self.camera.scale() <= 10.0 {
                        self.overlay_state.dragging_light = true;
                    }
                }
            }

            Input::MouseDragged(_start, new) => {
                // Move the light if we are dragging it
                if self.overlay_state.dragging_light {
                    self.light.set_pos(new);
                }
            },

            Input::MouseDragReleased => {
                self.overlay_state.dragging_light = false;
            },

            Input::Altitude(up) => {
                if self.overlay_state.dragging_light {
                    self.light.adjust_height(if up { 100.0 } else { -100.0 });
                }
            },

            Input::OverlayToggled => {
                self.overlay_state.toggle_overlay();
            },

            Input::CameraDragged(delta) => {
                self.camera.move_position(-delta);
            },

            Input::LightingToggled(toggle) => {
                if let Some(toggling_lighting) = toggle {
                    if toggling_lighting {
                        self.overlay_state.lighting_on = !self.overlay_state.lighting_on;
                    } else {
                        self.overlay_state.normals_on = !self.overlay_state.normals_on;
                    }
                } else {
                    // Go to fully shaded unless we are already at fully shaded
                    if self.overlay_state.lighting_on && self.overlay_state.normals_on {
                        self.overlay_state.lighting_on = false;
                        self.overlay_state.normals_on = false;
                    } else {
                        self.overlay_state.lighting_on = true;
                        self.overlay_state.normals_on = true;
                    }
                }
            },

            Input::NoInput => { },
        }
    }
}