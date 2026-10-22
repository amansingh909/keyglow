//! Reactive and Ripple: lights started by key presses.

use std::collections::VecDeque;
use std::f32::consts::PI;

use super::{put, rate, smoothstep, Board, Cover, Spark};

pub(super) fn reactive_life(duration_ms: u32) -> f64 {
    duration_ms as f64 / 1000.0
}

/// The pressed key starts at full colour and eases out to nothing.
pub(super) fn reactive(out: &mut Cover, live: &mut VecDeque<Spark>, duration_ms: u32, t: f64) {
    let life = reactive_life(duration_ms);
    live.retain(|s| t - s.t0 < life);
    for s in live.iter() {
        let p = ((t - s.t0) / life) as f32;
        if p >= 0.0 {
            put(out, s.row, s.col, s.color, 0.5 + 0.5 * (PI * p).cos());
        }
    }
}

/// Keys per second the ring travels.
fn ring_speed(speed: u8) -> f32 {
    rate(speed) * 40.0
}

/// Seconds until a ring reaches `reach` keys and is gone.
pub(super) fn ripple_life(speed: u8, reach: u8) -> f64 {
    reach as f64 / ring_speed(speed) as f64
}

/// A ring `thickness` keys wide (at half strength) grows from the pressed key and dims
/// as it goes, vanishing when it has travelled `reach` keys.
pub(super) fn ripple(
    out: &mut Cover,
    live: &mut VecDeque<Spark>,
    board: &Board,
    speed: u8,
    reach: u8,
    thickness: f32,
    t: f64,
) {
    let life = ripple_life(speed, reach);
    live.retain(|s| t - s.t0 < life);
    let (v, reach, half) = (ring_speed(speed), reach as f32, thickness / 2.0);
    for s in live.iter() {
        let age = (t - s.t0) as f32;
        if age < 0.0 {
            continue;
        }
        let radius = age * v;
        let fade = 1.0 - radius / reach;
        let (ox, oy) = board.pos[s.row][s.col];
        for (r, c, x, y) in board.cells() {
            let d = ((x - ox).powi(2) + (y - oy).powi(2)).sqrt();
            // Full inside the ring's core, easing to nothing half a key past its edge.
            let k = smoothstep(0.5 + half - (d - radius).abs());
            if k > 0.0 {
                put(out, r, c, s.color, k * fade);
            }
        }
    }
}
