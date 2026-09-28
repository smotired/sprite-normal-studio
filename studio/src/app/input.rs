use controller::{Axis, Input};
use eframe::egui::{Event, InputState, Key, Modifiers, Rect};

use crate::app::StudioApp;

impl StudioApp {
    // Convert egui inputs into something our controller can use.
    pub fn handle_input<'a>(&mut self, i: &'a mut InputState, (viewport_rect, ppp): (Rect, f32)) -> Box<dyn Iterator<Item = Input> + 'a> {
        // Get movement constants
        let movement = |shift: bool| { if shift { 100.0 } else { 10.0 } };
        let scale = |shift: bool| { if shift { 4f32 } else { 2f32 } };

        // Get mouse position in world space over the image. None if mouse is not over image.
        let (vw, vh) = self.renderer.size();
        let mouse_world = i.pointer.latest_pos()
            .filter(|pos| viewport_rect.contains(*pos))
            .map(|pos| {
                let local = (pos - viewport_rect.min) * ppp; // egui::Vec2, points -> local origin
                self.controller.screen_to_world((local.x, local.y), (vw as f32, vh as f32))
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
                            Input::CameraScale(1f32 / scale, mouse_world)
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
                        Key::H => {
                            if pressed && !repeat && modifiers.ctrl {
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
                                Input::CameraScale(1.0f32 / scale(modifiers.shift), mouse_world)
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
}