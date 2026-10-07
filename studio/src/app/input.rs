use controller::{Axis, Input, InputModifiers};
use eframe::egui::{Event, InputState, Key, Modifiers, PointerButton, Pos2, Rect, Response, Ui};
use studio_math::Vec2;

use crate::app::StudioApp;

trait IntoInputModifiers {
    fn convert(self) -> InputModifiers;
}

impl IntoInputModifiers for Modifiers {
    fn convert(self) -> InputModifiers {
        InputModifiers { shift: self.shift, ctrl: self.ctrl, alt: self.alt }
    }
}

impl StudioApp {
    // Convert egui inputs into something our controller can use.
    pub fn handle_app_input<'a>(&mut self, i: &'a mut InputState, (viewport_rect, ppp): (Rect, f32)) -> Box<dyn Iterator<Item = Input> + 'a> {
        // Get movement constants
        let movement = |shift: bool| { if shift { 100.0 } else { 10.0 } };
        let scale = |shift: bool| { if shift { 2 } else { 1 } };

        // Get mouse position in world space over the image. None if mouse is not over image.
        let viewport_size = self.renderer.size();
        let mouse_world = i.pointer.latest_pos()
            .filter(|pos| viewport_rect.contains(*pos))
            .map(|pos| {
                let local = (pos - viewport_rect.min) * ppp; // egui::Vec2, points -> local origin
                self.controller.screen_to_world(Vec2::from(local), viewport_size)
            });

        // Handle specific events
        Box::new(i.events.clone().into_iter().map(move |event| {
            match event {                
                // Mouse wheel event: Handles move or zoom depending on if shift is held
                Event::MouseWheel { delta, modifiers, .. } => {
                    if modifiers.shift {
                        let scale = scale(modifiers.ctrl);
                        if delta.y > 0.0 {
                            Input::CameraScale(scale, mouse_world)
                        } else if delta.y < 0.0 {
                            Input::CameraScale(-scale, mouse_world)
                        } else { Input::NoInput }
                    } else {
                        if modifiers.ctrl && delta.y != 0.0 {
                            Input::CameraMove(Axis::Horizontal, delta.y * 8.0)
                        } else if !modifiers.ctrl && delta.y != 0.0 {
                            Input::CameraMove(Axis::Vertical, -delta.y * 8.0)
                        } else { Input::NoInput }
                    }
                },

                // Key events
                Event::Key { key, pressed, repeat, modifiers, .. } => {
                    match key {
                        // Recenter camera
                        Key::Home => {
                            if pressed && !repeat {
                                Input::Recenter
                            } else { Input::NoInput }
                        }

                        // Camera movement
                        Key::W => {
                            if pressed {
                                Input::CameraMove(Axis::Vertical, -movement(modifiers.shift))
                            } else { Input::NoInput }
                        },
                        Key::A => {
                            if pressed {
                                Input::CameraMove(Axis::Horizontal, -movement(modifiers.shift))
                            } else { Input::NoInput }
                        },
                        Key::S => {
                            if pressed {
                                Input::CameraMove(Axis::Vertical, movement(modifiers.shift))
                            } else { Input::NoInput }
                        },
                        Key::D => {
                            if pressed {
                                Input::CameraMove(Axis::Horizontal, movement(modifiers.shift))
                            } else { Input::NoInput }
                        },

                        // Camera zoom
                        Key::Equals => {
                            if pressed && !repeat && modifiers.ctrl {
                                i.consume_key(Modifiers::CTRL, Key::Equals); // prevent app from also zooming
                                Input::CameraScale(scale(modifiers.shift), mouse_world)
                            } else { Input::NoInput }
                        },
                        Key::Minus => {
                            if pressed && !repeat && modifiers.ctrl {
                                i.consume_key(Modifiers::CTRL, Key::Minus); // prevent app from also zooming
                                Input::CameraScale(-scale(modifiers.shift), mouse_world)
                            } else { Input::NoInput }
                        },

                        // Altitude
                        Key::PageUp => {
                            if pressed {
                                Input::Altitude(true)
                            } else { Input::NoInput }
                        },
                        Key::PageDown => {
                            if pressed {
                                Input::Altitude(false)
                            } else { Input::NoInput }
                        },

                        // Action controls
                        Key::Escape => {
                            if pressed && !repeat {
                                Input::Cancel
                            } else { Input::NoInput }
                        },
                        Key::Delete => {
                            if pressed && !repeat {
                                Input::Delete
                            } else { Input::NoInput }
                        },

                        // Tool selection
                        Key::V => {
                            if pressed && !repeat {
                                if modifiers.shift {
                                    Input::ToolSelected(controller::EditorToolKind::Point)
                                } else {
                                    Input::ToolSelected(controller::EditorToolKind::Zone)
                                }
                            } else { Input::NoInput }
                        },
                        Key::P => {
                            if pressed && !repeat {
                                Input::ToolSelected(controller::EditorToolKind::Pen)
                            } else { Input::NoInput }
                        },

                        // Toggling lighting
                        Key::L => {
                            if pressed && !repeat {
                                if modifiers.ctrl {
                                    Input::LightingToggled(None) // Toggle both lighting and normal map
                                } else if modifiers.shift {
                                    Input::LightingToggled(Some(false)) // Toggle normal map
                                } else {
                                    Input::LightingToggled(Some(true)) // Toggle just lighting
                                }
                            } else { Input::NoInput }
                        },

                        // Toggling the overlay
                        Key::O => {
                            if pressed && !repeat {
                                Input::OverlayToggled
                            } else { Input::NoInput }
                        },

                        // Everything else
                        _ => Input::NoInput,
                    }
                },

                _ => Input::NoInput,
            }
        }))
    }

    // Handle inputs on the editor image itself
    pub fn handle_editor_input(&mut self, ui: &Ui, response: Response) ->  Vec<Input> {
        // Get information about the editor image window
        let ppp = ui.ctx().pixels_per_point();
        let rect = response.rect;

        // Screen-space Pos2 -> local physical-pixel coordinates -> world space coordinates
        let viewport_size = Vec2::new(response.rect.width(), response.rect.height());
        let to_world = |pos: Pos2| {
            let local = (pos - rect.min) * ppp;
            self.controller.screen_to_world(Vec2::from(local), viewport_size)
        };

        // Final list of events to process
        let mut inputs = Vec::new();

        // Handle click as determined by egui
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos() {
                let modifiers = ui.input(|i| i.modifiers);
                inputs.push(Input::MouseClicked(to_world(pos), modifiers.convert()));
            }

        // Handle starting drag
        if response.drag_started_by(PointerButton::Primary) {
            let (press_origin, modifiers) = ui.input(|i| (i.pointer.press_origin(), i.modifiers));
            if let Some(start_screen) = press_origin {
                inputs.push(Input::MouseDragStarted(to_world(start_screen), modifiers.convert()));
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

                    inputs.push(Input::MouseDragged(start_world, final_world, modifiers.convert()));
                }
        }

        // Handle stopping drag
        if response.drag_stopped_by(PointerButton::Primary) {
            let modifiers = ui.input(|i| i.modifiers);
            inputs.push(Input::MouseDragReleased(modifiers.convert()));
        }

        // Handle an active drag with the middle mouse button for camera
        if response.dragged_by(PointerButton::Middle) {
            let delta = response.drag_delta();

            // Convert the delta to world space by treating it like a point and then finding delta from whatever 0 is
            let world_origin = to_world(Pos2::ZERO);
            let delta_pos_in_world = to_world(Pos2::new(delta.x, delta.y));
            let world_delta = delta_pos_in_world - world_origin;

            inputs.push(Input::CameraDragged(world_delta));
        }

        inputs
    }
}