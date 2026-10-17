use serde::{Deserialize, Serialize};
use studio_math::Vec2;

use crate::Controller;

/// Frames per second new animations play at
pub const DEFAULT_FPS: f32 = 12.0;

/// One frame of an animation: a sprite, held for some number of ticks.
/// The sprite is stored by its top left pixel and is always the current sprite size, so changing the sprite size keeps the frames where they were.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Top left pixel of the sprite in the spritesheet
    pub origin: (u32, u32),

    /// How many ticks of the animation this frame lasts. At least 1.
    pub hold: u32,
}

/// A named, looping list of frames.
/// Changes to animations aren't recorded in the history, so they can't be undone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,

    /// Ticks per second
    pub fps: f32,

    pub frames: Vec<Frame>,
}

impl Animation {
    /// An empty animation with the default FPS
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), fps: DEFAULT_FPS, frames: vec![] }
    }

    /// How many ticks one loop of the animation lasts
    pub fn duration_ticks(&self) -> u32 {
        self.frames.iter().map(|frame| frame.hold.max(1)).sum()
    }

    /// The index of the frame shown a number of seconds after the animation started, looping. None if there are no frames.
    pub fn frame_at(&self, seconds: f64) -> Option<usize> {
        let total = self.duration_ticks();
        if total == 0 { return None; }
        let ticks = (seconds.max(0.0) * self.fps.max(0.0) as f64).floor() as u64;
        let mut tick = (ticks % total as u64) as u32;
        for (index, frame) in self.frames.iter().enumerate() {
            let hold = frame.hold.max(1);
            if tick < hold { return Some(index); }
            tick -= hold;
        }
        None
    }
}

/// The cell of the sprite grid that a pixel is in, as its column and row
pub fn cell_of(origin: (u32, u32), sprite_size: (u32, u32)) -> (u32, u32) {
    (origin.0 / sprite_size.0.max(1), origin.1 / sprite_size.1.max(1))
}

/// The top left pixel of a cell of the sprite grid
pub fn origin_of(cell: (u32, u32), sprite_size: (u32, u32)) -> (u32, u32) {
    (cell.0 * sprite_size.0, cell.1 * sprite_size.1)
}

/// The cell after another in reading order: right along the row, then down to the next one, then back to the top left
pub fn next_cell(cell: (u32, u32), (columns, rows): (u32, u32)) -> (u32, u32) {
    if cell.0 + 1 < columns { return (cell.0 + 1, cell.1); }
    if cell.1 + 1 < rows { return (0, cell.1 + 1); }
    (0, 0)
}

impl Controller {
    /// How many whole sprites fit across and down the spritesheet. Always at least one each way.
    pub fn sheet_cells(&self) -> (u32, u32) {
        let (sheet, sprite) = (self.sheet_size(), self.sprite_size());
        ((sheet.0 / sprite.0.max(1)).max(1), (sheet.1 / sprite.1.max(1)).max(1))
    }

    /// The top left corner and size of a frame's sprite in world space
    pub fn frame_rect(&self, frame: &Frame) -> (Vec2, Vec2) {
        let (width, height) = self.sprite_size();
        (Vec2::new(frame.origin.0 as f32, frame.origin.1 as f32), Vec2::new(width as f32, height as f32))
    }

    /// The animations in the workspace
    pub fn animations(&self) -> &[Animation] { &self.animations }

    /// The animations in the workspace, to edit them. Edits aren't recorded in the history.
    pub fn animations_mut(&mut self) -> &mut Vec<Animation> { &mut self.animations }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(origin: (u32, u32), hold: u32) -> Frame { Frame { origin, hold } }

    /// Frames last as many ticks as they are held for, and the animation loops
    #[test]
    fn frame_at_holds() {
        let animation = Animation { name: "idle".into(), fps: 10.0, frames: vec![frame((0, 0), 1), frame((16, 0), 3), frame((32, 0), 1)] };
        assert_eq!(animation.duration_ticks(), 5);
        let at = |seconds: f64| animation.frame_at(seconds);
        assert_eq!(at(0.0), Some(0));
        assert_eq!(at(0.15), Some(1));
        assert_eq!(at(0.35), Some(1));
        assert_eq!(at(0.45), Some(2));
        assert_eq!(at(0.55), Some(0));
        assert_eq!(at(1.25), Some(1));
    }

    /// An animation with no frames shows nothing
    #[test]
    fn frame_at_empty() {
        assert_eq!(Animation::new("empty").frame_at(3.0), None);
        assert_eq!(Animation::new("empty").fps, DEFAULT_FPS);
    }

    /// The next cell goes along the row, then wraps to the next row, then back to the start
    #[test]
    fn next_cell_wraps() {
        let cells = (3, 2);
        assert_eq!(next_cell((0, 0), cells), (1, 0));
        assert_eq!(next_cell((2, 0), cells), (0, 1));
        assert_eq!(next_cell((2, 1), cells), (0, 0));
        assert_eq!(next_cell((0, 0), (1, 1)), (0, 0));
    }

    /// Cells and origins convert both ways, and origins that aren't on a changed grid round down to a cell
    #[test]
    fn cells_and_origins() {
        assert_eq!(origin_of((2, 1), (16, 32)), (32, 32));
        assert_eq!(cell_of((32, 32), (16, 32)), (2, 1));
        assert_eq!(cell_of((32, 32), (24, 24)), (1, 1));
        assert_eq!(cell_of((5, 5), (0, 0)), (5, 5));
    }

    /// Animations survive a round trip through JSON
    #[test]
    fn serde_round_trip() {
        let animation = Animation { name: "punch".into(), fps: 8.0, frames: vec![frame((0, 16), 2)] };
        let json = serde_json::to_string(&animation).unwrap();
        assert_eq!(serde_json::from_str::<Animation>(&json).unwrap(), animation);
    }
}
