//! Starlight: random keys light up and fade.

use std::collections::VecDeque;
use std::f32::consts::PI;

use super::{pick, put, Board, Cover, LayerState, Spark, MAX_LIVE};
use crate::rng::Rng;
use crate::setup::Palette;

fn life(duration_ms: u32) -> f64 {
    duration_ms as f64 / 1000.0
}

/// Drops finished stars and starts the ones due by `t`, about `density / 100 * 20` a second.
/// Each new star is stamped with the moment it was due, not `t`, so a slow frame doesn't bunch them up.
pub(super) fn spawn(
    st: &mut LayerState,
    palette: &Palette,
    density: u8,
    duration_ms: u32,
    t: f64,
    board: &Board,
    rng: &mut Rng,
) {
    let life = life(duration_ms);
    st.live.retain(|s| t - s.t0 < life);
    let gap = 1.0 / (density as f64 / 100.0 * 20.0);
    let mut next = match st.next_star {
        Some(due) => due,
        None => t + gap * rng.next_f32() as f64,
    };
    // Stars due more than a lifetime ago would already be gone, so skip them (after a pause).
    next = next.max(t - life);
    while next <= t {
        // At the cap a star is skipped rather than cutting an older one short.
        if st.live.len() < MAX_LIVE {
            let (row, col) = free_cell(&st.live, board, rng);
            st.live.push_back(Spark { row, col, t0: next, color: pick(palette, rng) });
        }
        next += gap * (0.5 + rng.next_f32() as f64);
    }
    st.next_star = Some(next);
}

/// A random key with an LED, trying a few times for one without a star already.
fn free_cell(live: &VecDeque<Spark>, board: &Board, rng: &mut Rng) -> (usize, usize) {
    let mut cell = board.leds[rng.below(board.leds.len())];
    for _ in 0..3 {
        if !live.iter().any(|s| (s.row, s.col) == cell) {
            break;
        }
        cell = board.leds[rng.below(board.leds.len())];
    }
    cell
}

/// Each star rises from nothing to full and back on a smooth bell over its lifetime.
pub(super) fn draw(out: &mut Cover, live: &VecDeque<Spark>, duration_ms: u32, t: f64) {
    let life = life(duration_ms);
    for s in live {
        let p = ((t - s.t0) / life) as f32;
        if (0.0..1.0).contains(&p) {
            let k = (PI * p).sin();
            put(out, s.row, s.col, s.color, k * k);
        }
    }
}
