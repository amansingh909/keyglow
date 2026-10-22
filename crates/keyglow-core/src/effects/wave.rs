//! Wave and Wheel: colour bands sliding across the keyboard, or turning around its middle.

use super::{Board, Colors, Cover};

/// `direction` in degrees: 0 left to right, 90 bottom to top. `band` is one colour's width in keys.
pub(super) fn wave(out: &mut Cover, colors: &Colors, board: &Board, phase: f32, direction: u16, band: u8) {
    let a = (direction as f32).to_radians();
    // Key positions grow downward, so "up" is negative y.
    let (dx, dy) = (a.cos(), -a.sin());
    let period = band as f32 * colors.bands() as f32;
    for (r, c, x, y) in board.cells() {
        out[r][c] = (colors.cyclic((x * dx + y * dy) / period - phase), 1.0);
    }
}

/// One turn of the wheel shows every colour once.
pub(super) fn wheel(out: &mut Cover, colors: &Colors, board: &Board, phase: f32, clockwise: bool) {
    let (cx, cy) = board.centre;
    // With y pointing down, atan2's angle already grows clockwise on the keyboard.
    let turn = if clockwise { -phase } else { phase };
    for (r, c, x, y) in board.cells() {
        let angle = (y - cy).atan2(x - cx) / std::f32::consts::TAU;
        out[r][c] = (colors.cyclic(angle + turn), 1.0);
    }
}
