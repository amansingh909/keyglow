//! Turns a setup into frames: draws every layer and mixes them bottom to top.

use crate::color::lerp;
use crate::effects::{Board, Cover, LayerState, EMPTY_COVER};
use crate::frame::{scale, Frame, BLACK_FRAME};
use crate::rng::Rng;
use crate::setup::{Layer, Setup, COLS, ROWS};

/// The most time one frame moves an effect along: a long sleep or a clock step can't jump it.
const MAX_STEP: f64 = 0.5;

/// Draws a setup's layers. Deterministic: the same seed, presses and times give the same frames.
pub struct Engine {
    setup: Setup,
    /// One per layer, in the same order.
    states: Vec<LayerState>,
    rng: Rng,
    board: Board,
    /// When the last frame was drawn.
    last_t: Option<f64>,
}

impl Engine {
    pub fn new(setup: Setup, seed: u64) -> Engine {
        let mut rng = Rng::new(seed);
        let setup = setup.sanitized();
        let states = fresh_states(&setup, &mut rng);
        Engine { setup, states, rng, board: Board::new(), last_t: None }
    }

    /// Replaces the setup. A layer whose effect is the same kind as the layer that was at its
    /// place keeps what it was showing (stars, rings, lit keys, where its wave or breath had
    /// got to, its noise and random colours), so dragging a slider doesn't restart it.
    /// Layers whose kind changed, and new layers, start fresh.
    pub fn set_setup(&mut self, setup: Setup) {
        let setup = setup.sanitized();
        let mut old = std::mem::take(&mut self.states).into_iter();
        for (i, layer) in setup.layers.iter().enumerate() {
            let state = match (self.setup.layers.get(i), old.next()) {
                (Some(was), Some(mut state)) if was.effect.kind() == layer.effect.kind() => {
                    state.carry_over(&was.effect, &layer.effect, &mut self.rng);
                    state
                }
                _ => LayerState::new(&layer.effect, &mut self.rng),
            };
            self.states.push(state);
        }
        self.setup = setup;
    }

    /// A key at this grid cell went down at `t` (seconds on a monotonic clock).
    pub fn key_press(&mut self, row: usize, col: usize, t: f64) {
        if row >= ROWS || col >= COLS {
            return;
        }
        for (layer, state) in self.setup.layers.iter().zip(&mut self.states) {
            if shows(layer) {
                state.press(&layer.effect, row, col, t, &mut self.rng);
            }
        }
    }

    /// The frame at `t`, with the setup's brightness applied. Waves, breaths and the like move
    /// on by the time since the last frame (at most MAX_STEP), so a speed change only changes
    /// how fast they go from here on.
    pub fn render(&mut self, t: f64) -> Frame {
        let dt = self.last_t.map_or(0.0, |last| (t - last).clamp(0.0, MAX_STEP));
        self.last_t = Some(t);
        if self.setup.brightness == 0 {
            return BLACK_FRAME;
        }
        let mut frame = BLACK_FRAME;
        for (layer, state) in self.setup.layers.iter().zip(&mut self.states) {
            if shows(layer) {
                let mut cover = EMPTY_COVER;
                state.draw(&layer.effect, t, dt, &self.board, &mut self.rng, &mut cover);
                mix(&mut frame, &cover, layer.opacity as f32 / 100.0);
            }
        }
        scale(&frame, self.setup.brightness)
    }

    /// False when every frame from `t` on would be the same, so the caller can stop drawing.
    pub fn animating(&self, t: f64) -> bool {
        self.setup.brightness > 0
            && self.setup.layers.iter().zip(&self.states).any(|(l, s)| shows(l) && s.moving(&l.effect, t))
    }

    pub fn fps(&self) -> u32 {
        self.setup.fps
    }
}

fn fresh_states(setup: &Setup, rng: &mut Rng) -> Vec<LayerState> {
    setup.layers.iter().map(|l| LayerState::new(&l.effect, rng)).collect()
}

/// A layer that is off, or fully see-through, draws nothing.
fn shows(layer: &Layer) -> bool {
    layer.enabled && layer.opacity > 0
}

fn mix(frame: &mut Frame, cover: &Cover, opacity: f32) {
    for (frame_row, cover_row) in frame.iter_mut().zip(cover) {
        for (key, &(color, coverage)) in frame_row.iter_mut().zip(cover_row) {
            if coverage > 0.0 {
                *key = lerp(*key, color, coverage * opacity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;
    use crate::keymap::has_led;
    use crate::setup::{Effect, EffectKind, Layer, Palette, COLS, ROWS};

    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
    const GREEN: Rgb = Rgb { r: 0, g: 255, b: 0 };
    const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

    fn layer(effect: Effect) -> Layer {
        Layer { enabled: true, opacity: 100, effect }
    }
    fn solid(color: Rgb) -> Layer {
        layer(Effect::Static { color })
    }
    fn kind(k: EffectKind) -> Layer {
        layer(Effect::default_for(k))
    }
    fn setup(layers: Vec<Layer>) -> Setup {
        Setup { name: "test".into(), fps: 30, brightness: 100, layers }
    }
    fn cells() -> impl Iterator<Item = (usize, usize)> {
        (0..ROWS).flat_map(|r| (0..COLS).map(move |c| (r, c)))
    }
    fn lit(c: Rgb) -> bool {
        c != Rgb::BLACK
    }
    fn near(a: Rgb, b: Rgb, tol: i32) -> bool {
        let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tol;
        d(a.r, b.r) && d(a.g, b.g) && d(a.b, b.b)
    }
    /// Renders at 30 frames a second from `from` up to `to` (inclusive).
    fn frames(e: &mut Engine, from: f64, to: f64) -> Vec<Frame> {
        let n = ((to - from) * 30.0).round() as usize;
        (0..=n).map(|i| e.render(from + i as f64 / 30.0)).collect()
    }

    #[test]
    fn a_still_setup_is_not_animating() {
        let mut e = Engine::new(setup(vec![solid(RED), solid(BLUE)]), 1);
        assert!(!e.animating(0.0));
        e.render(0.0);
        e.key_press(2, 3, 0.0);
        assert!(!e.animating(0.1));
        assert!(Engine::new(Setup::owner_default(), 1).animating(0.0));
    }

    #[test]
    fn wave_moves_over_time() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Wave)]), 1);
        assert_ne!(e.render(0.0), e.render(1.0));
        assert!(e.animating(1.0));
    }

    #[test]
    fn wave_only_uses_its_colours() {
        let (a, b) = (Rgb::from_hex("#03FFFA").unwrap(), Rgb::from_hex("#55007F").unwrap());
        let within = |x: u8, p: u8, q: u8| p.min(q) <= x && x <= p.max(q);
        let mut e = Engine::new(setup(vec![kind(EffectKind::Wave)]), 1);
        let fs = frames(&mut e, 0.0, 10.0);
        for f in &fs {
            for (r, c) in cells() {
                let k = f[r][c];
                assert!(within(k.r, a.r, b.r) && within(k.g, a.g, b.g) && within(k.b, a.b, b.b), "{k:?} at {r},{c}");
            }
        }
        // Both ends of the band show up somewhere over time.
        assert!(fs.iter().any(|f| cells().any(|(r, c)| near(f[r][c], a, 8))));
        assert!(fs.iter().any(|f| cells().any(|(r, c)| near(f[r][c], b, 8))));
    }

    #[test]
    fn upper_layer_opacity_mixes() {
        let mut top = solid(BLUE);
        top.opacity = 50;
        let f = Engine::new(setup(vec![solid(RED), top]), 1).render(0.0);
        for (r, c) in cells() {
            assert!(near(f[r][c], Rgb { r: 128, g: 0, b: 127 }, 1), "{:?}", f[r][c]);
        }
    }

    #[test]
    fn disabled_layer_draws_nothing() {
        let mut top = solid(BLUE);
        top.enabled = false;
        let f = Engine::new(setup(vec![solid(RED), top]), 1).render(0.0);
        assert!(cells().all(|(r, c)| f[r][c] == RED));

        let mut wave = kind(EffectKind::Wave);
        wave.enabled = false;
        let mut e = Engine::new(setup(vec![wave]), 1);
        assert_eq!(e.render(1.0), BLACK_FRAME);
        assert!(!e.animating(1.0));
    }

    #[test]
    fn uncovered_keys_show_the_layer_below() {
        let mut e = Engine::new(setup(vec![solid(GREEN), kind(EffectKind::Starlight)]), 1);
        let f = e.render(0.0);
        assert!(cells().all(|(r, c)| f[r][c] == GREEN));
        // Later, stars light a few keys and the rest stay green.
        let f = frames(&mut e, 1.0 / 30.0, 3.0).pop().unwrap();
        let green = cells().filter(|&(r, c)| f[r][c] == GREEN).count();
        assert!(green > ROWS * COLS / 2 && green < ROWS * COLS, "{green} green");
    }

    #[test]
    fn ripple_spreads_then_fades() {
        let mut e = Engine::new(setup(vec![solid(Rgb::BLACK), kind(EffectKind::Ripple)]), 1);
        e.key_press(3, 5, 0.0);
        assert!(e.animating(0.0));
        let fs = frames(&mut e, 0.0, 4.0);
        assert!(fs.iter().any(|f| cells().any(|(r, c)| c.abs_diff(5) >= 3 && lit(f[r][c]))));
        assert_eq!(e.render(5.0), BLACK_FRAME);
        assert!(!e.animating(5.0));
    }

    #[test]
    fn ripple_is_a_ring_not_a_disc() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Ripple)]), 1);
        e.key_press(3, 9, 0.0);
        // Default speed 50 is about 8.8 keys a second, so at 0.4 s the ring is about 3.5 keys out.
        let f = e.render(0.4);
        assert!(!lit(f[3][9]), "the centre has gone dark");
        assert!(lit(f[3][9 + 3]) || lit(f[3][9 + 4]), "the ring is lit to the right");
        assert!(lit(f[3][9 - 3]) || lit(f[3][9 - 4]), "the ring is lit to the left");
    }

    #[test]
    fn reactive_lights_the_pressed_key() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Reactive)]), 1);
        e.key_press(2, 3, 0.0);
        let f = e.render(0.05);
        assert!(near(f[2][3], Rgb::WHITE, 20), "{:?}", f[2][3]);
        assert!(cells().filter(|&rc| rc != (2, 3)).all(|(r, c)| !lit(f[r][c])));
        assert!(e.animating(0.5));
        assert_eq!(e.render(1.0), BLACK_FRAME);
        assert!(!e.animating(1.0));
    }

    #[test]
    fn starlight_lights_some_keys_and_only_led_keys() {
        let stars = Effect::Starlight { palette: Palette::Random, density: 100, duration_ms: 1200 };
        let mut e = Engine::new(setup(vec![layer(stars)]), 1);
        let fs = frames(&mut e, 0.0, 0.5);
        for f in &fs {
            for (r, c) in cells() {
                assert!(has_led(r, c) || !lit(f[r][c]), "no-LED cell {r},{c} lit");
            }
            assert!((0..ROWS).all(|r| !lit(f[r][0])));
        }
        assert!(cells().any(|(r, c)| lit(fs.last().unwrap()[r][c])));
    }

    #[test]
    fn stars_fade_in_and_out() {
        let stars = Effect::Starlight { palette: Palette::Colors(vec![Rgb::WHITE]), density: 5, duration_ms: 2000 };
        let mut e = Engine::new(setup(vec![layer(stars)]), 3);
        let fs = frames(&mut e, 0.0, 20.0);
        // Some key passes through a dim level on its way up to (nearly) full and back down.
        let found = cells().any(|(r, c)| {
            let levels: Vec<u8> = fs.iter().map(|f| f[r][c].r).collect();
            levels.windows(3).any(|w| w[0] > 0 && w[0] < 100 && w[1] > w[0] && w[2] > w[1])
                && levels.iter().any(|&l| l > 200)
                && levels.windows(2).any(|w| w[0] < 100 && w[0] > 0 && w[1] < w[0])
        });
        assert!(found);
    }

    #[test]
    fn same_seed_gives_same_frames() {
        let run = |seed| {
            let mut e = Engine::new(Setup::owner_default(), seed);
            e.key_press(3, 5, 0.2);
            frames(&mut e, 0.0, 3.0)
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn brightness_zero_is_black() {
        let mut s = Setup::owner_default();
        s.brightness = 0;
        let mut e = Engine::new(s, 1);
        e.key_press(3, 5, 0.0);
        assert!(frames(&mut e, 0.0, 2.0).iter().all(|f| *f == BLACK_FRAME));
    }

    #[test]
    fn rate_runs_from_twenty_seconds_to_one() {
        use crate::effects::rate;
        assert!((rate(1) - 0.05).abs() < 1e-6);
        assert!((rate(100) - 1.0).abs() < 1e-5);
        assert!(rate(50) > rate(40));
    }

    #[test]
    fn presses_are_capped_and_expire() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Ripple), kind(EffectKind::Reactive)]), 1);
        for i in 0..1000 {
            e.key_press(i % ROWS, i % COLS, i as f64 * 0.001);
        }
        assert!(e.states.iter().all(|s| s.live_count() <= 64));
        e.render(100.0);
        assert!(e.states.iter().all(|s| s.live_count() == 0));
        assert!(!e.animating(100.0));
    }

    #[test]
    fn starlight_stars_survive_a_density_change() {
        let stars = |density| {
            layer(Effect::Starlight { palette: Palette::Colors(vec![Rgb::WHITE]), density, duration_ms: 2000 })
        };
        let mut e = Engine::new(setup(vec![stars(60)]), 1);
        let before = frames(&mut e, 0.0, 1.0).pop().unwrap();
        let lit_before: Vec<_> = cells().filter(|&(r, c)| lit(before[r][c])).collect();
        assert!(!lit_before.is_empty());
        e.set_setup(setup(vec![stars(61)]));
        let after = e.render(1.0 + 1.0 / 30.0);
        assert!(lit_before.iter().all(|&(r, c)| lit(after[r][c])), "stars were dropped");

        // Dragging the slider saves every 50 ms; the stars must still get past fading in.
        let mut lit_keys = 0;
        for i in 0..90 {
            if i % 2 == 0 {
                e.set_setup(setup(vec![stars(60 + (i / 2 % 2) as u8)]));
            }
            let f = e.render(2.0 + i as f64 / 30.0);
            lit_keys += cells().filter(|&(r, c)| f[r][c].r > 128).count();
        }
        assert!(lit_keys / 90 >= 3, "only {} bright keys a frame while dragging", lit_keys as f64 / 90.0);
    }

    #[test]
    fn raising_starlight_density_shows_soon() {
        let stars = |density| {
            layer(Effect::Starlight { palette: Palette::Colors(vec![Rgb::WHITE]), density, duration_ms: 2000 })
        };
        let mut e = Engine::new(setup(vec![stars(1)]), 1);
        frames(&mut e, 0.0, 0.2);
        e.set_setup(setup(vec![stars(100)]));
        let f = frames(&mut e, 0.2 + 1.0 / 30.0, 0.5).pop().unwrap();
        assert!(cells().any(|(r, c)| lit(f[r][c])), "no stars yet at the new density");
    }

    #[test]
    fn ripples_survive_an_opacity_change() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Ripple)]), 1);
        e.key_press(3, 9, 0.0);
        e.render(0.2);
        let mut faded = setup(vec![kind(EffectKind::Ripple)]);
        faded.layers[0].opacity = 80;
        e.set_setup(faded);
        assert!(e.animating(0.25));
        let f = e.render(0.25);
        assert!(cells().any(|(r, c)| lit(f[r][c])), "the ring in flight was dropped");
    }

    #[test]
    fn a_changed_effect_kind_starts_fresh() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Ripple)]), 1);
        e.key_press(3, 9, 0.0);
        e.render(0.1);
        e.set_setup(setup(vec![kind(EffectKind::Reactive)]));
        assert!(!e.animating(0.15));
        assert_eq!(e.render(0.15), BLACK_FRAME);
    }

    #[test]
    fn a_palette_change_shows_at_once() {
        let wave = |colors| layer(Effect::Wave { palette: Palette::Colors(colors), speed: 40, direction: 0, band: 18 });
        let mut e = Engine::new(setup(vec![wave(vec![RED, BLUE])]), 1);
        e.render(0.0);
        e.set_setup(setup(vec![wave(vec![GREEN])]));
        let f = e.render(1.0 / 30.0);
        assert!(cells().all(|(r, c)| f[r][c] == GREEN));
    }

    #[test]
    fn a_random_wave_keeps_its_colours_across_a_change() {
        let mut wave = layer(Effect::Wave { palette: Palette::Random, speed: 40, direction: 0, band: 18 });
        let mut e = Engine::new(setup(vec![wave.clone()]), 1);
        let before = e.render(0.0);
        wave.opacity = 99;
        e.set_setup(setup(vec![wave]));
        let after = e.render(0.0);
        assert!(biggest_step(&before, &after) <= 3, "{}", biggest_step(&before, &after));
    }

    fn with_speed(kind: EffectKind, speed: u8) -> Layer {
        let mut effect = Effect::default_for(kind);
        match &mut effect {
            Effect::Breathing { speed: s, .. }
            | Effect::Spectrum { speed: s }
            | Effect::Wave { speed: s, .. }
            | Effect::Wheel { speed: s, .. }
            | Effect::Fire { speed: s, .. } => *s = speed,
            _ => unreachable!(),
        }
        layer(effect)
    }

    /// The largest change of any channel of any key between two frames.
    fn biggest_step(a: &Frame, b: &Frame) -> u8 {
        let d = |x: u8, y: u8| x.abs_diff(y);
        cells()
            .map(|(r, c)| d(a[r][c].r, b[r][c].r).max(d(a[r][c].g, b[r][c].g)).max(d(a[r][c].b, b[r][c].b)))
            .max()
            .unwrap()
    }

    /// The average, over keys, of each key's largest channel change between two frames.
    fn mean_step(a: &Frame, b: &Frame) -> f32 {
        let d = |x: u8, y: u8| x.abs_diff(y);
        let sum: u32 = cells()
            .map(|(r, c)| d(a[r][c].r, b[r][c].r).max(d(a[r][c].g, b[r][c].g)).max(d(a[r][c].b, b[r][c].b)) as u32)
            .sum();
        sum as f32 / (ROWS * COLS) as f32
    }

    /// Runs `kind` at speed 40 for two seconds about an hour in, then nudges the speed to 41.
    /// Returns the frames before the change and the first frame after it. (At exactly 3600 s
    /// the old time-based phase drifted by a whole 18.00 cycles, which hid the jump.)
    fn speed_nudge(kind: EffectKind) -> (Vec<Frame>, Frame) {
        let mut e = Engine::new(setup(vec![with_speed(kind, 40)]), 1);
        let fs = frames(&mut e, 3698.0, 3700.0);
        e.set_setup(setup(vec![with_speed(kind, 41)]));
        let after = e.render(3700.0 + 1.0 / 30.0);
        (fs, after)
    }

    #[test]
    fn speed_change_does_not_jump_the_wave() {
        let (fs, after) = speed_nudge(EffectKind::Wave);
        let step = biggest_step(fs.last().unwrap(), &after);
        assert!(step <= 40, "jumped by {step}");
    }

    #[test]
    fn speed_change_does_not_jump_other_effects() {
        let mut jumps = Vec::new();
        for kind in [EffectKind::Breathing, EffectKind::Spectrum, EffectKind::Wheel] {
            let (fs, after) = speed_nudge(kind);
            let step = biggest_step(fs.last().unwrap(), &after);
            if step > 40 {
                jumps.push(format!("{kind:?} jumped by {step}"));
            }
        }
        // Fire's keys flicker hard from frame to frame, so compare its average change instead.
        let (fs, after) = speed_nudge(EffectKind::Fire);
        let usual = fs.windows(2).map(|w| mean_step(&w[0], &w[1])).fold(0.0, f32::max);
        let step = mean_step(fs.last().unwrap(), &after);
        if step > usual * 1.5 + 1.0 {
            jumps.push(format!("Fire changed by {step:.1} a key on average (usual at most {usual:.1})"));
        }
        assert!(jumps.is_empty(), "{jumps:?}");
    }

    #[test]
    fn a_long_sleep_or_a_step_back_moves_at_most_half_a_second() {
        let last = |times: &[f64]| {
            let mut e = Engine::new(setup(vec![kind(EffectKind::Wave)]), 1);
            times.iter().map(|&t| e.render(t)).last().unwrap()
        };
        assert_eq!(last(&[10.0, 10_000.0]), last(&[10.0, 10.5]));
        assert_eq!(last(&[10.0, 5.0]), last(&[10.0]));
    }

    #[test]
    fn breathing_fades_in_and_out() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Breathing)]), 1);
        let period = 1.0 / crate::effects::rate(30) as f64;
        // Frames come steadily, about 30 a second, through half a breath.
        let mut at = |i: u32| e.render(i as f64 * period / 240.0);
        assert_eq!(at(0), BLACK_FRAME);
        let quarter = (1..=60).map(&mut at).last().unwrap();
        assert!(quarter[1][1].g > 60 && quarter[1][1].g < 200, "{:?}", quarter[1][1]);
        let half = (61..=120).map(&mut at).last().unwrap();
        assert!(cells().all(|(r, c)| near(half[r][c], Rgb::from_hex("#03FFFA").unwrap(), 1)));
    }

    #[test]
    fn spectrum_paints_every_key_alike_and_changes() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Spectrum)]), 1);
        let (a, b) = (e.render(0.0), e.render(2.0));
        assert!(cells().all(|(r, c)| a[r][c] == a[0][0] && b[r][c] == b[0][0]));
        assert_ne!(a[0][0], b[0][0]);
    }

    #[test]
    fn wheel_turns() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Wheel)]), 1);
        let (a, b) = (e.render(0.0), e.render(1.0));
        assert_ne!(a, b);
        assert!(cells().any(|(r, c)| a[r][c] != a[0][1]));
    }

    #[test]
    fn fire_is_warm_hottest_at_the_bottom_and_flickers() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Fire)]), 1);
        let fs = frames(&mut e, 0.0, 4.0);
        let heat = |row: usize| -> u64 {
            fs.iter().flat_map(|f| f[row].iter()).map(|k| k.r as u64 + k.g as u64).sum()
        };
        assert!(heat(5) > heat(3) && heat(3) > heat(1), "{} {} {}", heat(5), heat(3), heat(1));
        for f in &fs {
            assert!(cells().all(|(r, c)| f[r][c].b == 0 && f[r][c].g <= f[r][c].r));
        }
        assert_ne!(fs[0], fs[3]);
    }

    #[test]
    fn huge_times_still_move() {
        let mut e = Engine::new(setup(vec![kind(EffectKind::Wave)]), 1);
        let t = 3.0e7; // about a year of uptime
        assert_ne!(e.render(t), e.render(t + 0.5));
    }

    #[test]
    #[ignore = "timing; run with --release --ignored --nocapture"]
    fn render_cost() {
        let slow_rings = Effect::Ripple { palette: Palette::Random, speed: 1, reach: 36, thickness: 6.0 };
        let cases = [
            ("owner default, a press every 10 frames", Setup::owner_default(), 10),
            ("fire", setup(vec![kind(EffectKind::Fire)]), 0),
            ("64 live slow rings over fire", setup(vec![kind(EffectKind::Fire), layer(slow_rings)]), 1),
        ];
        for (name, s, every) in cases {
            let mut e = Engine::new(s, 1);
            let start = std::time::Instant::now();
            for i in 0..10_000 {
                let t = i as f64 / 30.0;
                if every > 0 && i % every == 0 {
                    e.key_press(i % ROWS, i % COLS, t);
                }
                std::hint::black_box(e.render(t));
            }
            let el = start.elapsed();
            println!("{name}: 10000 renders in {el:?} ({:?} each)", el / 10_000);
        }
    }
}
