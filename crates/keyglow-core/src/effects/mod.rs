//! The nine effects. Each one fills a `Cover`: a colour and a coverage (0 to 1) for every cell.
//! The engine then mixes the covers bottom to top.

mod fire;
mod keys;
mod solid;
mod starlight;
mod wave;

use std::collections::VecDeque;

use crate::color::{gradient, hsv, lerp, Rgb};
use crate::keymap::{has_led, key_position};
use crate::rng::Rng;
use crate::setup::{Effect, Palette, COLS, ROWS};

pub(crate) type Grid<T> = [[T; COLS]; ROWS];

/// One layer's output for a frame: a colour and how much of it covers each cell.
pub(crate) type Cover = Grid<(Rgb, f32)>;

pub(crate) const EMPTY_COVER: Cover = [[(Rgb::BLACK, 0.0); COLS]; ROWS];

/// The most stars, lit keys or rings one layer keeps alive at once.
pub(crate) const MAX_LIVE: usize = 64;

/// Speed slider (1 to 100) to cycles per second: one cycle in 20 s at 1, one per second at 100.
pub(crate) fn rate(speed: u8) -> f32 {
    0.05 * 20f32.powf((speed as f32 - 1.0) / 99.0)
}

/// How far through the current cycle `t` is, 0 to 1. Worked out in f64 so a long uptime keeps its precision.
fn phase(t: f64, speed: u8) -> f32 {
    (t * rate(speed) as f64).rem_euclid(1.0) as f32
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A number in 0..1 that depends only on its inputs.
fn hash(salt: u64, a: u64) -> f32 {
    Rng::new(salt ^ a.wrapping_mul(0x9E37_79B9_7F4A_7C15)).next_f32()
}

fn random_colour(rng: &mut Rng) -> Rgb {
    hsv(rng.next_f32(), 1.0, 1.0)
}

/// A colour for a new star, key or ring: one of the list at random, or any hue.
fn pick(palette: &Palette, rng: &mut Rng) -> Rgb {
    match palette {
        Palette::Colors(list) if !list.is_empty() => list[rng.below(list.len())],
        _ => random_colour(rng),
    }
}

/// Keeps the stronger of what is already on a cell and a new light.
fn put(out: &mut Cover, row: usize, col: usize, color: Rgb, coverage: f32) {
    if coverage > out[row][col].1 {
        out[row][col] = (color, coverage.min(1.0));
    }
}

/// Things about the keyboard that never change: key centres, the cells with LEDs, the middle.
pub(crate) struct Board {
    pos: Grid<(f32, f32)>,
    leds: Vec<(usize, usize)>,
    centre: (f32, f32),
}

impl Board {
    pub(crate) fn new() -> Board {
        let mut pos = [[(0.0, 0.0); COLS]; ROWS];
        let mut leds = Vec::new();
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for (r, row) in pos.iter_mut().enumerate() {
            for (c, p) in row.iter_mut().enumerate() {
                *p = key_position(r, c);
                if has_led(r, c) {
                    leds.push((r, c));
                    lo = (lo.0.min(p.0), lo.1.min(p.1));
                    hi = (hi.0.max(p.0), hi.1.max(p.1));
                }
            }
        }
        Board { pos, leds, centre: ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0) }
    }

    fn cells(&self) -> impl Iterator<Item = (usize, usize, f32, f32)> + '_ {
        self.pos.iter().enumerate().flat_map(|(r, row)| row.iter().enumerate().map(move |(c, &(x, y))| (r, c, x, y)))
    }
}

/// The colours a full-keyboard effect (Wave, Wheel, Fire) paints with. A random palette is
/// rolled once per setup, so the wave doesn't change colour from frame to frame.
enum Colors {
    List(Vec<Rgb>),
    Rainbow,
}

impl Colors {
    fn of(palette: Option<&Palette>, rng: &mut Rng) -> Colors {
        match palette {
            Some(Palette::Colors(list)) => Colors::List(list.clone()),
            Some(Palette::Random) => Colors::List((0..3).map(|_| random_colour(rng)).collect()),
            Some(Palette::Rainbow) => Colors::Rainbow,
            None => Colors::List(Vec::new()),
        }
    }

    /// How many colour bands one full cycle holds; a rainbow counts as three.
    fn bands(&self) -> usize {
        match self {
            Colors::List(list) => list.len().max(1),
            Colors::Rainbow => 3,
        }
    }

    /// Wraps: 0 and 1 are both the first colour.
    fn cyclic(&self, u: f32) -> Rgb {
        match self {
            Colors::List(list) => gradient(list, u),
            Colors::Rainbow => hsv(u, 1.0, 1.0),
        }
    }

    /// Doesn't wrap: 0 is the first colour, 1 the last.
    fn ramp(&self, u: f32) -> Rgb {
        let u = u.clamp(0.0, 1.0);
        match self {
            Colors::List(list) if list.len() > 1 => {
                let p = u * (list.len() - 1) as f32;
                let i = (p.floor() as usize).min(list.len() - 2);
                lerp(list[i], list[i + 1], p - i as f32)
            }
            Colors::List(list) => list.first().copied().unwrap_or(Rgb::BLACK),
            Colors::Rainbow => hsv(u * 5.0 / 6.0, 1.0, 1.0),
        }
    }
}

/// A star, a lit key or a ring: where it started, when, and in what colour.
#[derive(Clone, Copy, Debug)]
struct Spark {
    row: usize,
    col: usize,
    t0: f64,
    color: Rgb,
}

/// What one layer remembers between frames.
pub(crate) struct LayerState {
    colors: Colors,
    /// Seeds the layer's own noise and per-breath colours.
    salt: u64,
    /// Oldest first, never more than MAX_LIVE.
    live: VecDeque<Spark>,
    /// When Starlight's next star is due.
    next_star: Option<f64>,
}

impl LayerState {
    pub(crate) fn new(effect: &Effect, rng: &mut Rng) -> LayerState {
        let transient = matches!(effect, Effect::Starlight { .. } | Effect::Reactive { .. } | Effect::Ripple { .. });
        LayerState {
            colors: Colors::of(effect.palette(), rng),
            salt: rng.next_u64(),
            live: VecDeque::with_capacity(if transient { MAX_LIVE } else { 0 }),
            next_star: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.live.len()
    }

    /// A key at (row, col) went down at `t`. Only Reactive and Ripple care.
    pub(crate) fn press(&mut self, effect: &Effect, row: usize, col: usize, t: f64, rng: &mut Rng) {
        let palette = match effect {
            Effect::Reactive { palette, .. } => {
                // Pressing a lit key again restarts it rather than stacking a second light.
                self.live.retain(|s| (s.row, s.col) != (row, col));
                palette
            }
            Effect::Ripple { palette, .. } => palette,
            _ => return,
        };
        if self.live.len() >= MAX_LIVE {
            self.live.pop_front();
        }
        self.live.push_back(Spark { row, col, t0: t, color: pick(palette, rng) });
    }

    /// Whether this layer will look different at some time after `t`.
    pub(crate) fn moving(&self, effect: &Effect, t: f64) -> bool {
        let lasts = match *effect {
            Effect::Static { .. } => return false,
            Effect::Reactive { duration_ms, .. } => keys::reactive_life(duration_ms),
            Effect::Ripple { speed, reach, .. } => keys::ripple_life(speed, reach),
            _ => return true,
        };
        self.live.iter().any(|s| s.t0 + lasts > t)
    }

    pub(crate) fn draw(&mut self, effect: &Effect, t: f64, board: &Board, rng: &mut Rng, out: &mut Cover) {
        match *effect {
            Effect::Static { color } => solid::fill(out, color),
            Effect::Breathing { ref palette, speed } => solid::breathing(out, palette, speed, self.salt, t),
            Effect::Spectrum { speed } => solid::fill(out, hsv(phase(t, speed), 1.0, 1.0)),
            Effect::Wave { speed, direction, band, .. } => {
                wave::wave(out, &self.colors, board, phase(t, speed), direction, band)
            }
            Effect::Wheel { speed, clockwise, .. } => wave::wheel(out, &self.colors, board, phase(t, speed), clockwise),
            Effect::Starlight { ref palette, density, duration_ms } => {
                starlight::spawn(self, palette, density, duration_ms, t, board, rng);
                starlight::draw(out, &self.live, duration_ms, t);
            }
            Effect::Reactive { duration_ms, .. } => keys::reactive(out, &mut self.live, duration_ms, t),
            Effect::Ripple { speed, reach, thickness, .. } => {
                keys::ripple(out, &mut self.live, board, speed, reach, thickness, t)
            }
            Effect::Fire { speed, height, .. } => fire::draw(out, &self.colors, board, speed, height, self.salt, t),
        }
    }
}
