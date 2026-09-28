use controller::{Axis, Input};
use eframe::egui::{Event, InputState, Key, Modifiers};

use crate::app::StudioApp;

impl StudioApp {
    // Convert egui inputs into something our controller can use.
    pub fn handle_input<'a>(&mut self, i: &'a mut InputState) -> Box<dyn Iterator<Item = Input> + 'a> {
        // Get movement constants
        let movement = |shift: bool| { if shift { 100.0 } else { 10.0 } };
        let scale = |shift: bool| { if shift { 4f32 } else { 2f32 } };

        // Handle specific events
        Box::new(i.events.clone().into_iter().map(move |event| {
            match event {
                // Mouse wheel event: Handles move or zoom depending on if ctrl is held
                Event::MouseWheel { delta, modifiers, .. } => {
                    if modifiers.ctrl {
                        let scale = scale(modifiers.shift);
                        if delta.y > 0.0 {
                            Input::CameraScale(scale)
                        } else if delta.y < 0.0 {
                            Input::CameraScale(1f32 / scale)
                        } else { Input::NoInput }
                    } else {
                        if modifiers.shift && delta.y != 0.0 {
                            Input::CameraMove(Axis::Horizontal, delta.y * 5.0)
                        } else if !modifiers.shift && delta.y != 0.0 {
                            Input::CameraMove(Axis::Vertical, -delta.y * 5.0)
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
                                Input::CameraScale(scale(modifiers.shift))
                            } else { Input::NoInput }
                        },
                        Key::Minus => {
                            if pressed && !repeat && modifiers.ctrl {
                                i.consume_key(Modifiers::CTRL, Key::Minus); // prevent app from also zooming
                                Input::CameraScale(1.0f32 / scale(modifiers.shift))
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