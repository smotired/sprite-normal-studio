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

    /// Cancel the current action (esc)
    Cancel,

    /// Delete key is pressed
    Delete,

    /// Overlay is toggled on or off. Maybe later I will add an enum for OverlayComponentKind which is passed here.
    OverlayToggled,

    /// Lighting/shading is toggled.
    /// If none, toggle both. If true, toggling lighting, otherwise toggling normal.
    LightingToggled(Option<bool>),
}

impl Controller {
    /// Set the cursor position in world space.
    pub fn set_cursor_pos(&mut self, pos: Vec2) {
        self.cursor_pos = pos;
    }

    // Get the closest control point to a click position, if it's in the click range corrected for camera scale.
    fn get_clicked_control_point(&self, zone_id: Option<u16>, pos: Vec2) -> Option<(u16, u16)> {
        if let Some((zone_id, point_id)) = self.objects.get_closest_point(zone_id, pos) {
            let distance = self.objects.get_point(point_id).unwrap().absolute_axis_distance(pos);
            if distance <= 5.0 * self.camera.inv_scale() { // size of control point boxes in the overlay, plus 2 pixels
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
            if point.left_handle().distance(pos) <= 5.0 * self.camera.inv_scale() {
                return Some(false); // left handle clicked
            }

            // Check the right handle
            if point.right_handle().distance(pos) <= 5.0 * self.camera.inv_scale() {
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
                if self.overlay_state.overlay_on() {
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
                                // If we clicked a point, see if we should end the path
                                if let Some((other_zone_id, point_id)) = self.get_clicked_control_point(Some(zone_id), pos) {
                                    // If it's the same zone, only do anything if we clicked the first point
                                    if other_zone_id == zone_id {
                                        let (first_point, count) = self.objects.get_zone(zone_id).unwrap().range();
                                        if point_id == first_point && count > 1 {
                                            let point = self.objects.get_point(point_id).unwrap();
                                            // If the point is continuous, update it to be broken and make its left handle linear
                                            if let ControlPointMode::Continuous = point.mode() {
                                                self.objects.update_point(point_id, None, Some(ControlPointMode::Broken), Some(Vec2::ZERO), None).unwrap();
                                            }
                                            self.selected_point = Some(point_id);

                                            // Go to the Zone tool. Should keep the zone selected.
                                            self.select_tool(EditorTool::Zone);
                                            self.normals_stale = true;
                                        }
                                    }

                                    // Otherwise, join the path if a path can be made to our original point via sibling points
                                    else {
                                        // TODO

                                        // Go to the Zone tool. Should keep the zone selected.
                                        self.select_tool(EditorTool::Zone);
                                        self.normals_stale = true;
                                    }
                                }

                                // Otherwise add a new linear point to the selected zone
                                else {
                                    self.selected_point = Some(self.objects.create_point(zone_id, pos).unwrap());
                                }
                            }
                            // Otherwise, we should try to create a zone by adding a linear point
                            else {
                                // TODO: Add sibling point if there is a point here
                                let zone_id = self.objects.create_zone().unwrap();
                                self.selected_zone = Some(zone_id);
                                self.selected_point = Some(self.objects.create_point(zone_id, pos).unwrap());
                            }
                        }
                    }
                }
            },

            Input::MouseDragStarted(pos) => {
                self.overlay_state.dragging_light = false;

                if self.overlay_state.overlay_on() {
                    // Decide if we should start dragging the light, which takes priority over the tool
                    if self.light.distance(pos) * self.camera.scale() <= 10.0 {
                        self.overlay_state.dragging_light = true;
                    }

                    // Check the tool
                    else {
                        match self.tool {
                            EditorTool::Zone => {
                                // If there is a control point here, select the zone
                                if let Some((zone_id, point_id)) = self.get_clicked_control_point(None, pos) {
                                    self.selected_zone = Some(zone_id);
                                    self.selected_point = Some(point_id);
                                }
                                // Otherwise deselect
                                else {
                                    self.selected_zone = None;
                                    self.selected_point = None;
                                }
                            },
                            EditorTool::Point => {
                                // If we selected a handle of the current control point, start dragging it
                                if let Some(right) = self.get_clicked_handle(pos) {
                                    self.dragging_handle = Some(right);
                                }

                                // Otherwise if there is a control point here, select it
                                else if let Some((zone_id, point_id)) = self.get_clicked_control_point(None, pos) {
                                    self.selected_zone = Some(zone_id);
                                    self.selected_point = Some(point_id);
                                }

                                // Otherwise deselect
                                else {
                                    self.selected_zone = None;
                                    self.selected_point = None;
                                }
                            },
                            EditorTool::Pen => {
                                // If we have a zone selected, we are already creating one, so add a continuous point
                                if let Some(zone_id) = self.selected_zone {
                                    // If we clicked a point, check if it's the first point of the zone
                                    if let Some((other_zone_id, point_id)) = self.get_clicked_control_point(Some(zone_id), pos) {
                                        // If it's the same zone, only do anything if we clicked the first point and have more than one point
                                        if other_zone_id == zone_id {
                                            let (first_point, count) = self.objects.get_zone(zone_id).unwrap().range();
                                            if point_id == first_point && count > 1 {
                                                let point = self.objects.get_point(point_id).unwrap();
                                                // If the point is linear, update it to be broken and select it. We will start dragging its left handle.
                                                if let ControlPointMode::Linear = point.mode() {
                                                    self.objects.update_point(point_id, None, Some(ControlPointMode::Broken), None, None).unwrap();
                                                }
                                                self.selected_point = Some(point_id);
                                                self.dragging_handle = Some(false);
                                            }
                                        }

                                        // Otherwise, join the path if a path can be made to our original point via sibling points
                                        else {
                                            // TODO
                                        }
                                    }

                                    // Otherwise add a new continuous point to the selected zone
                                    else {
                                        let point_id = self.objects.create_point(zone_id, pos).unwrap();
                                        self.selected_point = Some(point_id);
                                        self.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None).unwrap();
                                        self.dragging_handle = Some(true);
                                    }
                                }
                                // Otherwise, we should try to create a zone by adding a continuous point
                                else {
                                    // TODO: Add sibling point if there is a point here
                                    let zone_id = self.objects.create_zone().unwrap();
                                    self.selected_zone = Some(zone_id);
                                    let point_id = self.objects.create_point(zone_id, pos).unwrap();
                                    self.selected_point = Some(point_id);
                                    self.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None).unwrap();
                                    self.dragging_handle = Some(true);
                                }
                            },
                        };
                    }
                }
            }

            Input::MouseDragged(_start, new) => {
                // Move the light if we are dragging it
                if self.overlay_state.dragging_light {
                    self.light.set_pos(new);
                }

                // Otherwise switch based on tool
                else if self.overlay_state.overlay_on() {
                    match self.tool {
                        EditorTool::Zone => {
                            // Drag selected zone
                            if let Some(zone_id) = self.selected_zone {
                                let point_id = self.selected_point.unwrap_or(self.objects.get_zone(zone_id).unwrap().range().0);
                                self.objects.update_zone_position(point_id, new).unwrap();
                                self.normals_stale = true;
                            }
                        },
                        EditorTool::Point => {
                            // Drag selected point
                            if let Some(point_id) = self.selected_point {
                                // If we are dragging the handle, adjust that
                                if let Some(right) = self.dragging_handle {
                                    let point_pos = self.objects.get_point(point_id).unwrap().position();
                                    let (lh, rh) = if right { (None, Some(new - point_pos)) } else { (Some(new - point_pos), None) };
                                    self.objects.update_point(point_id, None, None, lh, rh).unwrap();
                                    self.normals_stale = true;
                                }
                                // Otherwise drag the point
                                else {
                                    self.objects.update_point(point_id, Some(new), None, None, None).unwrap();
                                    self.normals_stale = true;
                                }
                            }
                        },
                        EditorTool::Pen => {
                            // Assume we are dragging the handle of the currently selected point
                            if let Some(point_id) = self.selected_point {
                                let point_pos = self.objects.get_point(point_id).unwrap().position();
                                let (first_point_id, _) = self.objects.get_zone(self.selected_zone.unwrap()).unwrap().range();
                                let handle = new - point_pos;
                                let (lh, rh) = if point_id == first_point_id {
                                    if self.dragging_handle.unwrap() { (None, Some(handle)) } else { (Some(handle), None) }
                                } else {
                                    if self.dragging_handle.unwrap() { (Some(-handle), Some(handle)) } else { (Some(handle), Some(-handle)) }
                                };
                                self.objects.update_point(point_id, None, None, lh, rh).unwrap();
                            }
                        },
                    }
                }
            },

            Input::MouseDragReleased => {
                if self.overlay_state.dragging_light {
                    self.overlay_state.dragging_light = false;
                }

                else if self.overlay_state.overlay_on() {
                    match self.tool {
                        EditorTool::Zone => {
                            // Release drag on selected zone
                        },
                        EditorTool::Point => {
                            // Release drag on selected point
                            self.dragging_handle = None;
                        },
                        EditorTool::Pen => {
                            // Release drag on point creation
                            self.dragging_handle = None;
                            // If the selected point is the first point of the path, and the path has more than one point, finalize the path creation
                            if let Some(zone_id) = self.selected_zone && let Some(point_id) = self.selected_point {
                                let (start_point, point_count) = self.objects.get_zone(zone_id).unwrap().range();
                                if point_id == start_point && point_count > 1 {
                                    // Move to Zone tool. Should keep the start node in the path selected.
                                    self.select_tool(EditorTool::Zone);
                                    self.normals_stale = true;
                                }
                            }
                        },
                    }
                }
            },

            Input::Cancel => {
                // In pen tool, if we are creating a zone, delete the whole zone.
                match self.tool {
                    EditorTool::Pen => {
                        if let Some(zone_id) = self.selected_zone {
                            if let Ok(()) = self.objects.delete_zone(zone_id) {
                                self.selected_zone = None;
                                self.selected_point = None;
                                self.dragging_handle = None;
                            }
                        }
                    }

                    _ => { },
                }
            },

            Input::Delete => {
                match self.tool {
                    EditorTool::Zone => {
                        if let Some(zone_id) = self.selected_zone {
                            if let Ok(()) = self.objects.delete_zone(zone_id) {
                                self.selected_zone = None;
                                self.selected_point = None;
                                self.normals_stale = true;
                            }
                        }
                    },
                    EditorTool::Point => {
                        if let Some(point_id) = self.selected_point {
                            if let Ok(()) = self.objects.delete_point(point_id) {
                                self.selected_zone = None;
                                self.selected_point = None;
                                self.dragging_handle = None;
                                self.normals_stale = true;
                            }
                        }
                    },
                    // Delete the previous point in the path
                    EditorTool::Pen => {
                        if let Some(_) = self.selected_zone {
                            if let Ok(zone_deleted) = self.objects.delete_point_in_wip_path(self.selected_point.unwrap()) {
                                self.selected_point = Some(self.selected_point.unwrap() - 1); // should be the previous point
                                self.dragging_handle = None;
                                self.normals_stale = true;
                                if zone_deleted {
                                    self.selected_zone = None;
                                    self.selected_point = None;
                                }
                            } 
                        }
                    },
                }
            },

            Input::Altitude(up) => {
                if self.overlay_state.dragging_light {
                    self.light.adjust_height(if up { 100.0 } else { -100.0 });
                }

                // todo: maybe move a zone between layers?
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