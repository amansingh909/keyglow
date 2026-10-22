//! Static, Breathing and Spectrum: the whole keyboard in one colour.

use super::{hash, Cover};
use crate::color::{hsv, lerp, Rgb};
use crate::setup::Palette;

pub(super) fn fill(out: &mut Cover, color: Rgb) {
    for row in out.iter_mut() {
        row.fill((color, 1.0));
    }
}

/// Fades from black up to the colour and back once per cycle. With several colours each
/// breath takes the next one; a random palette rolls a new hue per breath.
pub(super) fn breathing(out: &mut Cover, palette: &Palette, cycles: f64, salt: u64) {
    let n = cycles.floor();
    let level = 0.5 - 0.5 * (std::f32::consts::TAU * (cycles - n) as f32).cos();
    let color = match palette {
        Palette::Colors(list) if !list.is_empty() => list[(n as i64).rem_euclid(list.len() as i64) as usize],
        Palette::Rainbow => hsv((n * 0.618_034).fract() as f32, 1.0, 1.0),
        _ => hsv(hash(salt, n as i64 as u64), 1.0, 1.0),
    };
    fill(out, lerp(Rgb::BLACK, color, level));
}
