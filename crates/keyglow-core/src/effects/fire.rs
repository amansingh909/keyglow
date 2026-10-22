//! Fire: flickering warm colours rising from the bottom row.
//!
//! Heat at a key is a fade from the bottom row up to `height` percent of the keyboard,
//! broken up by smooth noise that scrolls upward (the flames rise) and changes over
//! time (they flicker). The palette runs from cool (first colour) to hot (last).

use super::{hash, rate, smoothstep, Board, Colors, Cover};
use crate::color::{lerp, Rgb};

/// Noise repeats every this many units on its rising and flicker axes, so time can wrap
/// without a seam and stays precise in f32.
const PERIOD: f64 = 1024.0;
/// Noise units the flames rise per speed cycle.
const RISE: f64 = 14.0;
/// Noise units of change per speed cycle.
const FLICKER: f64 = 10.0;

pub(super) fn draw(out: &mut Cover, colors: &Colors, board: &Board, speed: u8, height: u8, salt: u64, t: f64) {
    let cycles = t * rate(speed) as f64;
    let rise = (cycles * RISE).rem_euclid(PERIOD) as f32;
    let z = (cycles * FLICKER).rem_euclid(PERIOD) as f32;
    let reach = height as f32 / 100.0;
    let (top, bottom) = (board.pos[0][0].1, board.pos[board.pos.len() - 1][0].1);
    for (r, c, x, y) in board.cells() {
        // 0 on the bottom row, 1 on the top row.
        let up = (bottom - y) / (bottom - top);
        let base = 1.0 - up / (reach + 0.05);
        let n = 0.65 * noise(salt, x * 0.55, y * 0.7 + rise, z)
            + 0.35 * noise(salt, x * 1.1, y * 1.4 + 2.0 * rise, 2.0 * z);
        // Stretch the noise (two octaves bunch up around 0.5) so tongues of flame stand out.
        let n = smoothstep((n - 0.2) / 0.6);
        let heat = (base + (n - 0.5) * 1.3).clamp(0.0, 1.0);
        let lit = smoothstep(heat / 0.35);
        out[r][c] = (lerp(Rgb::BLACK, colors.ramp(heat), lit), 1.0);
    }
}

fn lattice(salt: u64, x: i32, y: i32, z: i32) -> f32 {
    let key = (x as u64 & 0xFFFF) | ((y as u64 & 1023) << 16) | ((z as u64 & 1023) << 32);
    hash(salt, key)
}

/// Smooth value noise in 0..1.
fn noise(salt: u64, x: f32, y: f32, z: f32) -> f32 {
    let (x0, y0, z0) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (smoothstep(x - x0), smoothstep(y - y0), smoothstep(z - z0));
    let (x0, y0, z0) = (x0 as i32, y0 as i32, z0 as i32);
    let at = |dx, dy, dz| lattice(salt, x0 + dx, y0 + dy, z0 + dz);
    let mix = |a: f32, b: f32, f: f32| a + (b - a) * f;
    let plane = |dz| mix(mix(at(0, 0, dz), at(1, 0, dz), fx), mix(at(0, 1, dz), at(1, 1, dz), fx), fy);
    mix(plane(0), plane(1), fz)
}
