use studio_math::Vec2;

use crate::{Controller, EditorToolKind, PastePlacement, SpecialAction};
use super::history::DragStep;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Axis {
    Vertical,
    Horizontal,
}

#[derive(Default, Copy, Clone, Debug)]
pub struct InputModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl InputModifiers {
    pub fn none()       -> Self { Default::default() }
    pub fn shift()      -> Self { Self { shift: true,  ctrl: false, alt: false }}
    pub fn ctrl()       -> Self { Self { shift: false, ctrl: true,  alt: false }}
    pub fn alt()        -> Self { Self { shift: false, ctrl: false, alt: true  }}
    pub fn shift_ctrl() -> Self { Self { shift: true,  ctrl: true,  alt: false }}
    pub fn shift_alt()  -> Self { Self { shift: true,  ctrl: false, alt: true  }}
    pub fn ctrl_alt()   -> Self { Self { shift: false, ctrl: true,  alt: true  }}
    pub fn all()        -> Self { Self { shift: true,  ctrl: true,  alt: true  }}
}

/// Defines actions that can be performed on the controller, from raw input or from the UI
pub enum Action {
    /// No action, used so that we don't kill the iterator early
    None,

    /// Recentering camera to image center and scale 1
    Recenter,

    /// Moving the camera in a direction by a float amount
    CameraMove(Axis, f32),

    /// Centering the camera on a world space position, keeping its scale
    CameraCenter(Vec2),

    /// Changing the camera scale.
    /// Optionally, scale relative to a fixed anchor point in world space.
    CameraScale(i32, Option<Vec2>),

    /// Left-clicking the mouse at a world space position
    MouseClicked(Vec2, InputModifiers),

    /// Starting a drag event
    MouseDragStarted(Vec2, InputModifiers),

    /// Dragging the mouse while left clicking, from a start position to an end position.
    MouseDragged(Vec2, Vec2, InputModifiers),

    /// Releasing the mouse dragging
    MouseDragReleased(InputModifiers),

    /// Dragging the camera across this world space delta.
    CameraDragged(Vec2),

    /// Altitude is changed (i.e. page up/down is pressed). True if going up.
    Altitude(bool),

    /// Cancel the current action (esc)
    Cancel,

    /// Delete key is pressed
    Delete,

    /// Select everything the selected tool can select
    SelectAll,

    /// Copy the selected zones and paste them, without using the clipboard
    Duplicate(PastePlacement),

    /// Undo the latest action or drag
    Undo,

    /// Redo the latest undone action or drag
    Redo,

    /// Reassign every pixel to zones and regenerate the whole normal map
    RegenerateNormals,

    /// Overlay is toggled on or off. Maybe later I will add an enum for OverlayComponentKind which is passed here.
    OverlayToggled,

    /// Zone paths in the overlay are toggled on or off, which also hides shapes
    PathsToggled,

    /// Shapes in the overlay are toggled on or off. The shape tools always show them.
    ShapesToggled,

    /// A specific editor tool is selected.
    ToolSelected(EditorToolKind),

    /// An action from the UI that the selected tool handles in its own way.
    Special(SpecialAction),

    /// An arrow key is pressed: move the selection in a direction (a unit vector), by a step that depends on the modifiers.
    /// The bool is true if the key is being held and repeating.
    Nudge(Vec2, InputModifiers, bool),

    /// The lines between sprites in the overlay are toggled on or off
    SpriteGridToggled,

    /// Lighting/shading is toggled.
    /// If none, toggle both. If true, toggling lighting, otherwise toggling normal.
    LightingToggled(Option<bool>),
}

impl Controller {
    /// Set the cursor position in world space.
    pub fn set_cursor_pos(&mut self, pos: Vec2) {
        self.cursor_pos = pos;
    }

    /// If the sprite is lit by the light
    pub fn lighting_on(&self) -> bool { self.overlay_state.lighting_on() }

    /// If the sprite is shaded with the normal map
    pub fn normals_on(&self) -> bool { self.overlay_state.normals_on() }

    /// If the overlay is shown
    pub fn overlay_on(&self) -> bool { self.overlay_state.overlay_on() }

    /// If zone paths are shown in the overlay
    pub fn paths_on(&self) -> bool { self.overlay_state.paths_on() }

    /// If shapes are shown in the overlay outside the shape tools
    pub fn shapes_on(&self) -> bool { self.overlay_state.shapes_on() }

    /// Color of the light, packed as 0x00BBGGRR
    pub fn light_color(&self) -> u32 { self.light.packed_color() }

    /// Release any drag in progress, as if the mouse was let go. For when the project stops getting the mouse, e.g. its tab is switched away from.
    pub fn end_drag(&mut self) {
        if self.overlay_state.dragging_light || self.tool_dragging {
            self.handle_action(Action::MouseDragReleased(InputModifiers::none()));
        }
    }

    /// If the lines between sprites are shown in the overlay
    pub fn sprite_grid_on(&self) -> bool { self.overlay_state.sprite_grid_on() }

    // Handle different actions from the UI
    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Recenter => {
                let center = Vec2::from(self.output().size()) * 0.5;
                self.camera.reset_position(center);
                self.light.set_pos(center);
            },

            Action::CameraMove(axis, amount) => {
                match axis {
                    Axis::Vertical => {
                        self.camera.move_position(Vec2::vt(amount * self.camera.inv_scale()));
                    },
                    Axis::Horizontal => {
                        self.camera.move_position(Vec2::hz(amount * self.camera.inv_scale()));
                    },
                }
            },

            Action::CameraCenter(pos) => {
                self.camera.position = pos;
            },

            Action::CameraScale(amount, relative_to) => {
                self.camera.apply_scale(amount, relative_to);
            },

            Action::MouseClicked(pos, modifiers) => {
                if self.overlay_state.overlay_on() {
                    self.run_tool(DragStep::None, |tool, state| tool.handle_click(state, pos, modifiers));
                }
            },

            Action::MouseDragStarted(pos, modifiers) => {
                self.overlay_state.dragging_light = false;

                if self.overlay_state.overlay_on() {
                    // Decide if we should start dragging the light, which takes priority over the tool
                    if self.light.distance(pos) * self.camera.scale() <= 10.0 {
                        self.overlay_state.dragging_light = true;
                    }

                    // Check the tool
                    else {
                        self.tool_dragging = true;
                        self.run_tool(DragStep::None, |tool, state| tool.handle_drag_start(state, pos, modifiers));
                    }
                }
            }

            Action::MouseDragged(_start, new, modifiers) => {
                // Move the light if we are dragging it
                if self.overlay_state.dragging_light {
                    self.light.set_pos(new);
                }

                // Otherwise continue the tool's drag, even if the overlay was hidden partway through it
                else if self.tool_dragging {
                    self.run_tool(DragStep::Continue, |tool, state| tool.handle_dragging_to(state, new, modifiers));
                }
            },

            Action::MouseDragReleased(modifiers) => {
                if self.overlay_state.dragging_light {
                    self.overlay_state.dragging_light = false;
                }

                else if self.tool_dragging {
                    self.tool_dragging = false;
                    self.run_tool(DragStep::Release, |tool, state| tool.handle_drag_released(state, modifiers));
                }
            },

            Action::Cancel => {
                self.run_tool(DragStep::None, |tool, state| tool.handle_cancel(state));
            },

            Action::Delete => {
                if self.tool_dragging { return; }
                self.run_tool(DragStep::None, |tool, state| tool.handle_delete(state));
            },

            Action::SelectAll => {
                if self.tool_dragging { return; }
                self.run_tool(DragStep::None, |tool, state| tool.handle_select_all(state));
            },

            Action::Duplicate(placement) => {
                if let Some(objects) = self.copy_selection() {
                    self.paste(&objects, placement);
                }
            },

            Action::Undo => {
                self.undo();
            },

            Action::Redo => {
                self.redo();
            },

            Action::RegenerateNormals => {
                self.mark_normals_stale(Some((Vec2::ZERO, self.assigner.size())));
            },

            Action::Altitude(up) => {
                if self.overlay_state.dragging_light {
                    self.light.adjust_height(if up { 100.0 } else { -100.0 });
                }
            },

            Action::OverlayToggled => {
                self.overlay_state.toggle_overlay();
            },

            Action::PathsToggled => {
                self.overlay_state.toggle_paths();
            },

            Action::ShapesToggled => {
                self.overlay_state.toggle_shapes();
            },

            Action::ToolSelected(tool) => {
                self.select_tool(tool);
            },

            Action::CameraDragged(delta) => {
                self.camera.move_position(-delta);
            },

            Action::LightingToggled(toggle) => {
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

            Action::Special(action) => {
                self.run_special_action(action);
            },

            Action::Nudge(direction, modifiers, repeat) => {
                let delta = self.nudge_delta(direction, modifiers);
                self.run_special_action(SpecialAction::SelectionNudged(delta, repeat));
            },

            Action::SpriteGridToggled => {
                self.overlay_state.toggle_sprite_grid();
            },

            Action::None => { },
        }
    }
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use crate::{Action, Controller, EditorToolKind, InputModifiers};
    use crate::objects::test_utils::{add_square, add_zone};

    /// A controller on a device with no GPU behind it, with a square at (0, 0) to (10, 10) and another zone off to the side.
    /// The light is moved away so drags never grab it.
    fn controller() -> Controller {
        let (device, _) = wgpu::Device::noop(&Default::default());
        let mut controller = Controller::new(&device);
        add_square(&mut controller.objects, 10.0);
        add_zone(&mut controller.objects, &[(50.0, 50.0), (60.0, 50.0), (60.0, 60.0)]);
        controller.light.set_pos(Vec2::new(-1000.0, -1000.0));
        controller
    }

    fn drag_start(controller: &mut Controller, pos: Vec2) { controller.handle_action(Action::MouseDragStarted(pos, InputModifiers::none())); }
    fn drag_to(controller: &mut Controller, pos: Vec2) { controller.handle_action(Action::MouseDragged(Vec2::ZERO, pos, InputModifiers::none())); }
    fn release(controller: &mut Controller) { controller.handle_action(Action::MouseDragReleased(InputModifiers::none())); }

    /// Actions that would change the objects or the tool under a drag are ignored until it is released
    #[test]
    fn actions_ignored_while_dragging() {
        let mut controller = controller();
        let other_zone = controller.objects.save().zones[1].clone();

        drag_start(&mut controller, Vec2::new(10.0, 10.0));
        drag_to(&mut controller, Vec2::new(15.0, 12.0));
        for action in [Action::Delete, Action::Undo, Action::SelectAll, Action::ToolSelected(EditorToolKind::Point)] {
            controller.handle_action(action);
        }
        drag_to(&mut controller, Vec2::new(20.0, 20.0));
        release(&mut controller);

        assert_eq!(controller.objects.zone_count(), 2);
        assert_eq!(controller.selected_tool(), EditorToolKind::Zone);
        assert_eq!(controller.objects.get_point_info(2).unwrap().position(), Vec2::new(20.0, 20.0));
        assert_eq!(controller.objects.save().zones[1], other_zone);

        // Once released, they work again
        controller.handle_action(Action::Delete);
        assert_eq!(controller.objects.zone_count(), 1);
    }

    /// Hiding the overlay partway through a drag still finishes it, so it is one undoable change
    #[test]
    fn overlay_hidden_mid_drag() {
        let mut controller = controller();
        drag_start(&mut controller, Vec2::new(10.0, 10.0));
        controller.handle_action(Action::OverlayToggled);
        drag_to(&mut controller, Vec2::new(20.0, 20.0));
        release(&mut controller);
        assert_eq!(controller.objects.get_point_info(2).unwrap().position(), Vec2::new(20.0, 20.0));

        controller.undo();
        assert_eq!(controller.objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
        assert!(!controller.can_undo());
    }

    /// Ending a drag from outside, like switching tabs, releases it
    #[test]
    fn end_drag_releases() {
        let mut controller = controller();
        drag_start(&mut controller, Vec2::new(10.0, 10.0));
        drag_to(&mut controller, Vec2::new(20.0, 20.0));
        controller.end_drag();
        assert!(!controller.tool_dragging);
        controller.undo();
        assert_eq!(controller.objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
    }
}
