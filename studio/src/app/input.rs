use controller::{Axis, Action, InputModifiers, SpecialAction};
use eframe::egui::{Event, InputState, Key, Modifiers, PointerButton, Pos2, Rect, Response, Ui};
use studio_math::Vec2;

use crate::app::StudioApp;
use crate::app::workspace::WorkspaceCommand;

trait IntoInputModifiers {
    fn convert(self) -> InputModifiers;
}

impl IntoInputModifiers for Modifiers {
    fn convert(self) -> InputModifiers {
        InputModifiers { shift: self.shift, ctrl: self.ctrl, alt: self.alt }
    }
}

impl StudioApp {
    /// Forward app-level input events to the controller and update the world space cursor position.
    /// Returns the workspace command whose shortcut was pressed, if any.
    pub(super) fn process_app_input(&mut self, ui: &mut Ui, viewport_rect: Rect) -> Option<WorkspaceCommand> {
        ui.input_mut(|i| {
            // Workspace shortcuts are consumed first, so their keys don't also do something else
            let command = WorkspaceCommand::ALL.into_iter().find(|command| i.consume_shortcut(&command.shortcut()));

            for event in self.handle_app_input(i, viewport_rect) {
                self.controller.handle_action(event);
            }

            // Set world space cursor position
            let viewport_size = Vec2::from(viewport_rect.size());
            if let Some(mouse_world) = i.pointer.latest_pos()
                .filter(|pos| viewport_rect.contains(*pos))
                .map(|pos| {
                    let local = pos - viewport_rect.min; // egui::Vec2, points relative to the viewport origin
                    self.controller.screen_to_world(Vec2::from(local), viewport_size)
                })
            {
                self.controller.set_cursor_pos(mouse_world);
            }

            command
        })
    }

    /// Forward input events on the editor image, i.e. click and drag, to the controller
    pub(super) fn process_editor_input(&mut self, ui: &Ui, response: Response) {
        for event in self.handle_editor_input(ui, response) {
            self.controller.handle_action(event);
        }
    }

    // Convert egui inputs into something our controller can use.
    pub fn handle_app_input<'a>(&mut self, i: &'a mut InputState, viewport_rect: Rect) -> Box<dyn Iterator<Item = Action> + 'a> {
        // Get movement constants
        let movement = |shift: bool| { if shift { 100.0 } else { 10.0 } };
        let scale = |shift: bool| { if shift { 2 } else { 1 } };

        // Get mouse position in world space over the image. None if mouse is not over image.
        let viewport_size = Vec2::from(viewport_rect.size());
        let mouse_world = i.pointer.latest_pos()
            .filter(|pos| viewport_rect.contains(*pos))
            .map(|pos| {
                let local = pos - viewport_rect.min; // egui::Vec2, points relative to the viewport origin
                self.controller.screen_to_world(Vec2::from(local), viewport_size)
            });

        // Handle specific events
        Box::new(i.events.clone().into_iter().map(move |event| {
            match event {                
                // Ignore scrolling outside the workspace, e.g. in the sidebar
                Event::MouseWheel { .. } if mouse_world.is_none() => Action::None,

                // Mouse wheel event: Handles move or zoom depending on if shift is held
                Event::MouseWheel { delta, modifiers, .. } => {
                    if modifiers.shift {
                        let scale = scale(modifiers.ctrl);
                        if delta.y > 0.0 {
                            Action::CameraScale(scale, mouse_world)
                        } else if delta.y < 0.0 {
                            Action::CameraScale(-scale, mouse_world)
                        } else { Action::None }
                    } else {
                        if modifiers.ctrl && delta.y != 0.0 {
                            Action::CameraMove(Axis::Horizontal, delta.y * 8.0)
                        } else if !modifiers.ctrl && delta.y != 0.0 {
                            Action::CameraMove(Axis::Vertical, -delta.y * 8.0)
                        } else { Action::None }
                    }
                },

                // Key events
                Event::Key { key, pressed, repeat, modifiers, .. } => {
                    match key {
                        // Recenter camera
                        Key::Home => {
                            if pressed && !repeat {
                                Action::Recenter
                            } else { Action::None }
                        }

                        // Camera movement
                        Key::W => {
                            if pressed {
                                Action::CameraMove(Axis::Vertical, -movement(modifiers.shift))
                            } else { Action::None }
                        },
                        Key::A => {
                            if pressed {
                                Action::CameraMove(Axis::Horizontal, -movement(modifiers.shift))
                            } else { Action::None }
                        },
                        Key::S => {
                            if pressed && !modifiers.command {
                                Action::CameraMove(Axis::Vertical, movement(modifiers.shift))
                            } else { Action::None }
                        },
                        Key::D => {
                            if pressed {
                                Action::CameraMove(Axis::Horizontal, movement(modifiers.shift))
                            } else { Action::None }
                        },

                        // Camera zoom
                        Key::Equals => {
                            if pressed && !repeat && modifiers.ctrl {
                                i.consume_key(Modifiers::CTRL, Key::Equals); // prevent app from also zooming
                                Action::CameraScale(scale(modifiers.shift), mouse_world)
                            } else { Action::None }
                        },
                        Key::Minus => {
                            if pressed && !repeat && modifiers.ctrl {
                                i.consume_key(Modifiers::CTRL, Key::Minus); // prevent app from also zooming
                                Action::CameraScale(-scale(modifiers.shift), mouse_world)
                            } else { Action::None }
                        },

                        // Altitude
                        Key::PageUp => {
                            if pressed {
                                Action::Altitude(true)
                            } else { Action::None }
                        },
                        Key::PageDown => {
                            if pressed {
                                Action::Altitude(false)
                            } else { Action::None }
                        },

                        // Action controls
                        Key::Escape => {
                            if pressed && !repeat {
                                Action::Cancel
                            } else { Action::None }
                        },
                        Key::Delete => {
                            if pressed && !repeat {
                                Action::Delete
                            } else { Action::None }
                        },

                        // Undo, redo, or point the selected zones straight out
                        Key::Z => {
                            if pressed && modifiers.ctrl {
                                if modifiers.shift { Action::Redo } else { Action::Undo }
                            } else if pressed && !repeat && modifiers.shift {
                                Action::Special(SpecialAction::NormalSet(studio_math::Vec3::FORWARD, false))
                            } else { Action::None }
                        },
                        Key::Y => {
                            if pressed && modifiers.ctrl {
                                if modifiers.shift { Action::Undo } else { Action::Redo }
                            } else { Action::None }
                        },

                        // Toggle the selected point between linear and continuous
                        Key::B => {
                            if pressed && !repeat && modifiers.ctrl {
                                Action::Special(SpecialAction::PointModeToggled)
                            } else { Action::None }
                        },

                        // Merge the selected points
                        Key::M => {
                            if pressed && !repeat && modifiers.ctrl {
                                Action::Special(SpecialAction::PointsMerged)
                            } else { Action::None }
                        },

                        // Toggle if branching zones in the pen tool inherit their normal
                        Key::I => {
                            if pressed && !repeat {
                                Action::Special(SpecialAction::ToggleInherit)
                            } else { Action::None }
                        },

                        // Toggle if the pen tool stays active after finishing a zone
                        Key::C => {
                            if pressed && !repeat && !modifiers.any() {
                                Action::Special(SpecialAction::ToggleStayActive)
                            } else { Action::None }
                        },

                        // Separate the selected zones or points from their siblings
                        Key::K => {
                            if pressed && !repeat && modifiers.ctrl {
                                Action::Special(SpecialAction::SelectionSeparated)
                            } else { Action::None }
                        },

                        // Tool selection
                        Key::V => {
                            if pressed && !repeat {
                                if modifiers.shift {
                                    Action::ToolSelected(controller::EditorToolKind::Point)
                                } else {
                                    Action::ToolSelected(controller::EditorToolKind::Zone)
                                }
                            } else { Action::None }
                        },
                        Key::P => {
                            if pressed && !repeat {
                                Action::ToolSelected(controller::EditorToolKind::Pen)
                            } else { Action::None }
                        },

                        // Toggling lighting
                        Key::L => {
                            if pressed && !repeat {
                                if modifiers.ctrl {
                                    Action::LightingToggled(None) // Toggle both lighting and normal map
                                } else if modifiers.shift {
                                    Action::LightingToggled(Some(false)) // Toggle normal map
                                } else {
                                    Action::LightingToggled(Some(true)) // Toggle just lighting
                                }
                            } else { Action::None }
                        },

                        // Toggling the overlay
                        Key::O => {
                            if pressed && !repeat && !modifiers.command {
                                Action::OverlayToggled
                            } else { Action::None }
                        },

                        // Everything else
                        _ => Action::None,
                    }
                },

                _ => Action::None,
            }
        }))
    }

    // Handle inputs on the editor image itself
    pub fn handle_editor_input(&mut self, ui: &Ui, response: Response) ->  Vec<Action> {
        // Get information about the editor image window
        let rect = response.rect;

        // Screen-space Pos2 -> points relative to the viewport origin -> world space coordinates
        let viewport_size = Vec2::new(rect.width(), rect.height());
        let to_world = |pos: Pos2| {
            let local = pos - rect.min;
            self.controller.screen_to_world(Vec2::from(local), viewport_size)
        };

        // Final list of events to process
        let mut inputs = Vec::new();

        // Handle click as determined by egui
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos() {
                let modifiers = ui.input(|i| i.modifiers);
                inputs.push(Action::MouseClicked(to_world(pos), modifiers.convert()));
            }

        // Handle starting drag
        if response.drag_started_by(PointerButton::Primary) {
            let (press_origin, modifiers) = ui.input(|i| (i.pointer.press_origin(), i.modifiers));
            if let Some(start_screen) = press_origin {
                inputs.push(Action::MouseDragStarted(to_world(start_screen), modifiers.convert()));
            }
        }

        // Handle an active drag
        if response.dragged_by(PointerButton::Primary) {
            let (press_origin, modifiers) = ui.input(|i| (i.pointer.press_origin(), i.modifiers));
            if let Some(start_screen) = press_origin
                && let Some(total_delta) = response.total_drag_delta() {
                    let start_world = to_world(start_screen);

                    // drag_delta is this frame's incremental movement
                    let final_screen = start_screen + total_delta;
                    let final_world = to_world(final_screen);

                    inputs.push(Action::MouseDragged(start_world, final_world, modifiers.convert()));
                }
        }

        // Handle stopping drag
        if response.drag_stopped_by(PointerButton::Primary) {
            let modifiers = ui.input(|i| i.modifiers);
            inputs.push(Action::MouseDragReleased(modifiers.convert()));
        }

        // Handle an active drag with the middle mouse button for camera
        if response.dragged_by(PointerButton::Middle) {
            let delta = response.drag_delta();

            // Convert the delta to world space by treating it like a point and then finding delta from whatever 0 is
            let world_origin = to_world(Pos2::ZERO);
            let delta_pos_in_world = to_world(Pos2::new(delta.x, delta.y));
            let world_delta = delta_pos_in_world - world_origin;

            inputs.push(Action::CameraDragged(world_delta));
        }

        inputs
    }
}