use studio_math::Vec2;

use crate::{Controller, InputModifiers};

/// Sizes in pixels the grid can be set to. The grid is what dragging snaps to while holding ctrl, and how far the arrow keys move the selection.
pub const GRID_SIZES: [u32; 5] = [1, 2, 4, 8, 16];

impl Controller {
    /// Size in pixels of the grid that ctrl snaps to and the arrow keys move by
    pub fn grid_size(&self) -> u32 { self.grid_size }

    /// Set the grid size, rounding down to the closest of GRID_SIZES
    pub fn set_grid_size(&mut self, size: u32) {
        self.grid_size = GRID_SIZES.into_iter().rev().find(|&option| option <= size).unwrap_or(GRID_SIZES[0]);
    }

    /// Width and height of each sprite the spritesheet is sliced into
    pub fn sprite_size(&self) -> (u32, u32) { self.sprite_size }

    /// Width and height of the spritesheet, or (0, 0) before one is loaded
    pub fn sheet_size(&self) -> (u32, u32) { self.sheet_size }

    /// Set the size of each sprite, clamped to fit in the spritesheet. (0, 0) means the whole spritesheet is one sprite.
    pub fn set_sprite_size(&mut self, size: (u32, u32)) {
        self.sprite_size = clamp_sprite_size(size, self.sheet_size());
        self.revision += 1;
    }

    /// How far the arrow keys move the selection in a direction, given the modifiers held.
    pub(crate) fn nudge_delta(&self, direction: Vec2, modifiers: InputModifiers) -> Vec2 {
        nudge_delta(direction, modifiers, self.grid_size, self.sprite_size)
    }
}

/// Clamp a sprite size to fit in a spritesheet. (0, 0) becomes the whole spritesheet.
/// Before a spritesheet is loaded its size is (0, 0), and the size is kept as it is so it can be clamped once one is.
fn clamp_sprite_size(size: (u32, u32), sheet: (u32, u32)) -> (u32, u32) {
    if size == (0, 0) { return sheet; }
    if sheet == (0, 0) { return size; }
    (size.0.clamp(1, sheet.0.max(1)), size.1.clamp(1, sheet.1.max(1)))
}

/// How far the arrow keys move the selection in a direction.
/// SHIFT: one sprite (its width horizontally and its height vertically)
/// CTRL: one pixel
/// Otherwise one grid space. Shift takes priority over ctrl.
fn nudge_delta(direction: Vec2, modifiers: InputModifiers, grid_size: u32, sprite_size: (u32, u32)) -> Vec2 {
    let step = if modifiers.shift {
        Vec2::new(sprite_size.0 as f32, sprite_size.1 as f32)
    } else if modifiers.ctrl {
        Vec2::new(1.0, 1.0)
    } else {
        Vec2::new(grid_size as f32, grid_size as f32)
    };
    Vec2::new(direction.x * step.x, direction.y * step.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each modifier moves by its own step, per axis, and shift beats ctrl
    #[test]
    fn nudge_steps() {
        let (grid, sprite) = (4, (32, 48));
        let right = Vec2::new(1.0, 0.0);
        let down = Vec2::new(0.0, 1.0);
        assert_eq!(nudge_delta(right, InputModifiers::none(), grid, sprite), Vec2::new(4.0, 0.0));
        assert_eq!(nudge_delta(-down, InputModifiers::ctrl(), grid, sprite), Vec2::new(0.0, -1.0));
        assert_eq!(nudge_delta(right, InputModifiers::shift(), grid, sprite), Vec2::new(32.0, 0.0));
        assert_eq!(nudge_delta(down, InputModifiers::shift(), grid, sprite), Vec2::new(0.0, 48.0));
        assert_eq!(nudge_delta(down, InputModifiers::shift_ctrl(), grid, sprite), Vec2::new(0.0, 48.0));
    }

    /// Sprite sizes fit in the spritesheet, and nothing set means the whole spritesheet
    #[test]
    fn sprite_size_clamped() {
        assert_eq!(clamp_sprite_size((0, 0), (256, 128)), (256, 128));
        assert_eq!(clamp_sprite_size((64, 200), (256, 128)), (64, 128));
        assert_eq!(clamp_sprite_size((0, 32), (256, 128)), (1, 32));
        assert_eq!(clamp_sprite_size((64, 32), (0, 0)), (64, 32));
    }
}
