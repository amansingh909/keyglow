//! Every rule of the settings window that is not drawing: layer order, the save throttle,
//! the background program's state and the unsaved marker.

use keyglow_core::color::Rgb;
use keyglow_core::setup::{Effect, EffectKind, Layer, Palette, Setup};
use keyglow_core::store::Status;
use std::time::{Duration, Instant};

// ---- Layers -------------------------------------------------------------------------------
//
// `Setup::layers` is stored bottom first; the window shows it top first. "Up" and "down" below
// are in screen terms: up means towards the top of the stack, which is the end of `layers`.

/// Adds a layer with the effect's default settings on top of the stack. Returns its index.
pub fn add_layer(setup: &mut Setup, kind: EffectKind) -> usize {
    setup.layers.push(Layer { enabled: true, opacity: 100, effect: Effect::default_for(kind) });
    setup.layers.len() - 1
}

/// Removes the layer (an out of range index removes nothing). Returns the layer to select next:
/// the one that now sits where the deleted one was on screen, or `None` when no layers are left.
pub fn delete_layer(setup: &mut Setup, index: usize) -> Option<usize> {
    if index < setup.layers.len() {
        setup.layers.remove(index);
    }
    if setup.layers.is_empty() {
        None
    } else {
        Some(index.saturating_sub(1).min(setup.layers.len() - 1))
    }
}

/// Moves a layer one step towards the top of the stack. Returns its new index.
pub fn move_up(setup: &mut Setup, index: usize) -> usize {
    if index + 1 < setup.layers.len() {
        setup.layers.swap(index, index + 1);
        index + 1
    } else {
        index
    }
}

/// Moves a layer one step towards the bottom of the stack. Returns its new index.
pub fn move_down(setup: &mut Setup, index: usize) -> usize {
    if index > 0 && index < setup.layers.len() {
        setup.layers.swap(index, index - 1);
        index - 1
    } else {
        index
    }
}

/// Layer indexes in the order the window lists them: top of the stack first.
pub fn display_order(setup: &Setup) -> Vec<usize> {
    (0..setup.layers.len()).rev().collect()
}

// ---- Effect settings ----------------------------------------------------------------------

/// The ways a layer's colours can be given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteChoice {
    Colors,
    Random,
    Rainbow,
}

impl PaletteChoice {
    pub fn of(palette: &Palette) -> PaletteChoice {
        match palette {
            Palette::Colors(_) => PaletteChoice::Colors,
            Palette::Random => PaletteChoice::Random,
            Palette::Rainbow => PaletteChoice::Rainbow,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PaletteChoice::Colors => "Colours",
            PaletteChoice::Random => "Random",
            PaletteChoice::Rainbow => "Rainbow",
        }
    }
}

/// Which palette kinds make sense for an effect. Empty for effects without a palette.
pub fn palette_choices(kind: EffectKind) -> &'static [PaletteChoice] {
    use PaletteChoice::*;
    match kind {
        EffectKind::Wave | EffectKind::Wheel => &[Colors, Rainbow],
        EffectKind::Breathing | EffectKind::Starlight | EffectKind::Reactive | EffectKind::Ripple => &[Colors, Random],
        EffectKind::Fire => &[Colors],
        EffectKind::Static | EffectKind::Spectrum => &[],
    }
}

/// The fewest and most colours a layer's colour list may hold. A gradient needs two colours;
/// Breathing fades between at most two.
pub fn color_limits(kind: EffectKind) -> (usize, usize) {
    match kind {
        EffectKind::Wave | EffectKind::Wheel => (2, 16),
        EffectKind::Breathing => (1, 2),
        _ => (1, 16),
    }
}

/// The palette to use after picking `choice`. Picking Colours brings back `remembered` colours
/// (from before the switch) or else the effect's default colours.
pub fn palette_for_choice(choice: PaletteChoice, kind: EffectKind, remembered: Option<&[Rgb]>) -> Palette {
    match choice {
        PaletteChoice::Random => Palette::Random,
        PaletteChoice::Rainbow => Palette::Rainbow,
        PaletteChoice::Colors => {
            if let Some(colors) = remembered.filter(|c| !c.is_empty()) {
                return Palette::Colors(colors.to_vec());
            }
            match Effect::default_for(kind).palette() {
                Some(Palette::Colors(colors)) => Palette::Colors(colors.clone()),
                _ => Palette::Colors(vec![Rgb { r: 0x03, g: 0xFF, b: 0xFA }, Rgb { r: 0x55, g: 0x00, b: 0x7F }]),
            }
        }
    }
}

fn palette_mut(effect: &mut Effect) -> Option<&mut Palette> {
    match effect {
        Effect::Static { .. } | Effect::Spectrum { .. } => None,
        Effect::Breathing { palette, .. }
        | Effect::Wave { palette, .. }
        | Effect::Wheel { palette, .. }
        | Effect::Starlight { palette, .. }
        | Effect::Reactive { palette, .. }
        | Effect::Ripple { palette, .. }
        | Effect::Fire { palette, .. } => Some(palette),
    }
}

/// Trims or pads a colour list to what `kind` allows.
fn fit_colors(mut colors: Vec<Rgb>, kind: EffectKind) -> Vec<Rgb> {
    let (min, max) = color_limits(kind);
    colors.truncate(max);
    while colors.len() < min {
        colors.push(next_new_color(&colors));
    }
    colors
}

/// A layer's effect switched to `kind`: that effect's defaults, keeping the old colours when
/// the new effect can use them.
pub fn change_effect(old: &Effect, kind: EffectKind) -> Effect {
    let mut new = Effect::default_for(kind);
    let carried = match old {
        Effect::Static { color } => Some(Palette::Colors(vec![*color])),
        _ => old.palette().cloned(),
    };
    let Some(carried) = carried else { return new };
    if let Effect::Static { color } = &mut new {
        if let Palette::Colors(colors) = &carried {
            if let Some(first) = colors.first() {
                *color = *first;
            }
        }
    } else if let Some(slot) = palette_mut(&mut new) {
        if palette_choices(kind).contains(&PaletteChoice::of(&carried)) {
            *slot = match carried {
                Palette::Colors(colors) => Palette::Colors(fit_colors(colors, kind)),
                other => other,
            };
        }
    }
    new
}

/// A colour for the "add colour" button: the first of a few bright colours not already in the list.
pub fn next_new_color(existing: &[Rgb]) -> Rgb {
    const CHOICES: [&str; 8] = ["#FFFFFF", "#FF0000", "#FF7F00", "#FFFF00", "#00FF00", "#03FFFA", "#0000FF", "#FF00FF"];
    CHOICES.iter().filter_map(|h| Rgb::from_hex(h)).find(|c| !existing.contains(c)).unwrap_or(Rgb::WHITE)
}

/// Quick direction buttons for Wave, in degrees (0 left to right, 90 bottom to top,
/// 180 right to left, 270 top to bottom). Each names where the wave travels to.
pub const DIRECTIONS: [(&str, u16); 4] = [("Left", 180), ("Right", 0), ("Up", 90), ("Down", 270)];

// ---- Saving -------------------------------------------------------------------------------

/// Limits how often `active.json` is written while a slider is dragged: at most once per
/// `min_gap`, and always once more after the last change (the caller keeps asking while dirty,
/// waking up after `wait`).
pub struct SaveThrottle {
    pub min_gap: Duration,
    last: Option<Instant>,
}

impl SaveThrottle {
    pub fn new(min_gap: Duration) -> SaveThrottle {
        SaveThrottle { min_gap, last: None }
    }

    /// `dirty` means there are changes not yet written. True means write them now.
    pub fn should_save(&mut self, now: Instant, dirty: bool) -> bool {
        if !dirty || !self.wait(now).is_zero() {
            return false;
        }
        self.last = Some(now);
        true
    }

    /// How long until a pending change may be written.
    pub fn wait(&self, now: Instant) -> Duration {
        match self.last {
            Some(t) => self.min_gap.saturating_sub(now.saturating_duration_since(t)),
            None => Duration::ZERO,
        }
    }
}

impl Default for SaveThrottle {
    fn default() -> Self {
        SaveThrottle::new(Duration::from_millis(50))
    }
}

/// True when there is no saved setup of the active setup's name, or it differs.
pub fn is_unsaved(active: &Setup, saved: Option<&Setup>) -> bool {
    saved != Some(active)
}

/// Whether switching to another setup would lose edits, so the window should ask first. The
/// untouched owner default never needs asking about: the setup menu can bring it back.
pub fn ask_before_switching(active: &Setup, saved: Option<&Setup>) -> bool {
    is_unsaved(active, saved) && *active != Setup::owner_default()
}

/// The store's rules for a setup name (it becomes a file name), with a reason to show when broken.
/// The name is checked as typed; callers trim it first.
pub fn check_setup_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("Type a name.".into())
    } else if name.chars().count() > 64 {
        Err("Names can be at most 64 characters long.".into())
    } else if name.contains('/') || name.contains('\0') {
        Err("Names cannot contain a slash.".into())
    } else if name.starts_with('.') {
        Err("Names cannot start with a dot.".into())
    } else {
        Ok(())
    }
}

// ---- The background program ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaemonState {
    NotRunning,
    NoKeyboard,
    Ready,
}

/// The status file is only trusted while the process that wrote it is still alive.
pub fn daemon_state(status: Option<Status>, pid_alive: impl Fn(u32) -> bool) -> DaemonState {
    match status {
        Some(s) if pid_alive(s.pid) => {
            if s.keyboard {
                DaemonState::Ready
            } else {
                DaemonState::NoKeyboard
            }
        }
        _ => DaemonState::NotRunning,
    }
}

/// Whether `pid` is a running `keyglowd` (a reused pid of some other program does not count).
pub fn keyglowd_alive(pid: u32) -> bool {
    pid != 0
        && std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .is_ok_and(|comm| comm.trim_end().starts_with("keyglowd"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyglow_core::setup::Setup;

    fn kinds_top_first(s: &Setup) -> Vec<EffectKind> {
        display_order(s).into_iter().map(|i| s.layers[i].effect.kind()).collect()
    }

    #[test]
    fn add_puts_the_new_layer_on_top() {
        let mut s = Setup::owner_default();
        let i = add_layer(&mut s, EffectKind::Fire);
        assert_eq!(i, 3);
        assert_eq!(s.layers.len(), 4);
        assert_eq!(s.layers[i].effect.kind(), EffectKind::Fire);
        assert!(s.layers[i].enabled);
        assert_eq!(s.layers[i].opacity, 100);
        assert_eq!(kinds_top_first(&s)[0], EffectKind::Fire);

        let mut empty = Setup { layers: vec![], ..Setup::owner_default() };
        assert_eq!(add_layer(&mut empty, EffectKind::Static), 0);
    }

    #[test]
    fn display_order_is_top_first() {
        let s = Setup::owner_default();
        assert_eq!(display_order(&s), vec![2, 1, 0]);
        assert_eq!(kinds_top_first(&s), [EffectKind::Ripple, EffectKind::Starlight, EffectKind::Wave]);
        let empty = Setup { layers: vec![], ..Setup::owner_default() };
        assert!(display_order(&empty).is_empty());
    }

    #[test]
    fn moving_past_the_ends_is_clamped() {
        let mut s = Setup::owner_default();
        // Up on screen means towards the top of the stack, the end of `layers`.
        assert_eq!(move_up(&mut s, 0), 1);
        assert_eq!(kinds_top_first(&s), [EffectKind::Ripple, EffectKind::Wave, EffectKind::Starlight]);
        assert_eq!(move_up(&mut s, 1), 2);
        assert_eq!(kinds_top_first(&s), [EffectKind::Wave, EffectKind::Ripple, EffectKind::Starlight]);
        // Already on top: nothing moves.
        assert_eq!(move_up(&mut s, 2), 2);
        assert_eq!(kinds_top_first(&s), [EffectKind::Wave, EffectKind::Ripple, EffectKind::Starlight]);
        assert_eq!(move_down(&mut s, 2), 1);
        assert_eq!(kinds_top_first(&s), [EffectKind::Ripple, EffectKind::Wave, EffectKind::Starlight]);
        // Already at the bottom: nothing moves.
        assert_eq!(move_down(&mut s, 0), 0);
        assert_eq!(kinds_top_first(&s), [EffectKind::Ripple, EffectKind::Wave, EffectKind::Starlight]);
        // Out of range indexes change nothing.
        let before = s.clone();
        assert_eq!(move_up(&mut s, 9), 9);
        assert_eq!(move_down(&mut s, 9), 9);
        assert_eq!(s, before);
    }

    #[test]
    fn delete_removes_the_layer_and_picks_the_next_one_down_on_screen() {
        let mut s = Setup::owner_default();
        // Deleting Starlight (index 1) selects Wave, the layer below it on screen.
        assert_eq!(delete_layer(&mut s, 1), Some(0));
        assert_eq!(kinds_top_first(&s), [EffectKind::Ripple, EffectKind::Wave]);
        // Deleting the bottom layer selects the new bottom.
        assert_eq!(delete_layer(&mut s, 0), Some(0));
        assert_eq!(kinds_top_first(&s), [EffectKind::Ripple]);
        assert_eq!(delete_layer(&mut s, 5), Some(0));
        assert_eq!(delete_layer(&mut s, 0), None);
        assert!(s.layers.is_empty());
        assert_eq!(delete_layer(&mut s, 0), None);
    }

    #[test]
    fn throttle_saves_at_most_every_50ms_and_after_the_last_change() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let mut t = SaveThrottle::new(ms(50));
        assert_eq!(t.min_gap, ms(50));
        // Nothing to save: never saves.
        assert!(!t.should_save(t0, false));
        // The first change saves at once.
        assert!(t.should_save(t0, true));
        // Changes while dragging inside the gap wait.
        assert!(!t.should_save(t0 + ms(10), true));
        assert!(!t.should_save(t0 + ms(49), true));
        // Once the gap has passed, the pending change is written.
        assert!(t.should_save(t0 + ms(50), true));
        assert!(!t.should_save(t0 + ms(60), true));
        // The last change (still dirty, no more input) is written once the gap passes...
        assert_eq!(t.wait(t0 + ms(60)), ms(40));
        assert!(t.should_save(t0 + ms(100), true));
        // ...and then nothing more until something changes again.
        assert!(!t.should_save(t0 + ms(200), false));
        assert!(!t.should_save(t0 + ms(5000), false));
        // After a quiet spell the next change is written at once.
        assert_eq!(t.wait(t0 + ms(5000)), Duration::ZERO);
        assert!(t.should_save(t0 + ms(5000), true));
    }

    #[test]
    fn unsaved_until_it_matches_the_saved_file() {
        let active = Setup::owner_default();
        assert!(is_unsaved(&active, None));
        let saved = active.clone();
        assert!(!is_unsaved(&active, Some(&saved)));
        let mut changed = active.clone();
        changed.layers[0].opacity = 50;
        assert!(is_unsaved(&changed, Some(&saved)));
        let mut back = changed.clone();
        back.layers[0].opacity = 100;
        assert!(!is_unsaved(&back, Some(&saved)));
        let mut renamed = active.clone();
        renamed.name = "Other".into();
        assert!(is_unsaved(&renamed, Some(&saved)));
    }

    #[test]
    fn daemon_state_needs_a_live_pid() {
        let st = |keyboard| Some(Status { pid: 42, keyboard, detail: String::new() });
        let alive = |pid: u32| pid == 42;
        let dead = |_: u32| false;
        assert_eq!(daemon_state(None, alive), DaemonState::NotRunning);
        assert_eq!(daemon_state(st(true), dead), DaemonState::NotRunning);
        assert_eq!(daemon_state(st(false), alive), DaemonState::NoKeyboard);
        assert_eq!(daemon_state(st(true), alive), DaemonState::Ready);
    }

    #[test]
    fn only_a_running_keyglowd_counts_as_alive() {
        assert!(!keyglowd_alive(0));
        assert!(!keyglowd_alive(u32::MAX));
        // This test process is alive but is not keyglowd.
        assert!(!keyglowd_alive(std::process::id()));
    }

    #[test]
    fn palette_choices_follow_the_effect() {
        use PaletteChoice::*;
        assert_eq!(palette_choices(EffectKind::Wave), [Colors, Rainbow]);
        assert_eq!(palette_choices(EffectKind::Wheel), [Colors, Rainbow]);
        for k in [EffectKind::Breathing, EffectKind::Starlight, EffectKind::Reactive, EffectKind::Ripple] {
            assert_eq!(palette_choices(k), [Colors, Random], "{k:?}");
        }
        assert_eq!(palette_choices(EffectKind::Fire), [Colors]);
        assert!(palette_choices(EffectKind::Static).is_empty());
        assert!(palette_choices(EffectKind::Spectrum).is_empty());
        // Every default palette is one of the allowed choices.
        for k in EffectKind::ALL {
            if let Some(p) = Effect::default_for(k).palette() {
                assert!(palette_choices(k).contains(&PaletteChoice::of(p)), "{k:?}");
            }
        }
    }

    #[test]
    fn switching_back_to_colours_restores_them() {
        let pink = Rgb { r: 255, g: 0, b: 127 };
        assert_eq!(palette_for_choice(PaletteChoice::Random, EffectKind::Starlight, None), Palette::Random);
        assert_eq!(palette_for_choice(PaletteChoice::Rainbow, EffectKind::Wave, None), Palette::Rainbow);
        assert_eq!(
            palette_for_choice(PaletteChoice::Colors, EffectKind::Starlight, Some(&[pink])),
            Palette::Colors(vec![pink])
        );
        // Nothing remembered: the effect's default colours, or a fallback pair.
        assert_eq!(
            palette_for_choice(PaletteChoice::Colors, EffectKind::Wave, Some(&[])),
            Effect::default_for(EffectKind::Wave).palette().unwrap().clone()
        );
        match palette_for_choice(PaletteChoice::Colors, EffectKind::Wheel, None) {
            Palette::Colors(c) => assert!(c.len() >= color_limits(EffectKind::Wheel).0),
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn new_colours_are_not_already_in_the_list() {
        assert_eq!(next_new_color(&[]), Rgb::WHITE);
        let first = next_new_color(&[Rgb::WHITE]);
        assert_ne!(first, Rgb::WHITE);
        assert_ne!(next_new_color(&[Rgb::WHITE, first]), first);
    }

    #[test]
    fn changing_the_effect_keeps_colours_that_fit() {
        let c = |h: &str| Rgb::from_hex(h).unwrap();
        let wave = Effect::Wave {
            palette: Palette::Colors(vec![c("#FF0000"), c("#00FF00"), c("#0000FF")]),
            speed: 9,
            direction: 90,
            band: 4,
        };
        // Wave to Wheel: same colours, Wheel's other defaults.
        match change_effect(&wave, EffectKind::Wheel) {
            Effect::Wheel { palette, speed, .. } => {
                assert_eq!(palette, Palette::Colors(vec![c("#FF0000"), c("#00FF00"), c("#0000FF")]));
                assert_eq!(speed, 30);
            }
            e => panic!("{e:?}"),
        }
        // Breathing takes at most two colours.
        assert_eq!(
            change_effect(&wave, EffectKind::Breathing).palette(),
            Some(&Palette::Colors(vec![c("#FF0000"), c("#00FF00")]))
        );
        // Static takes the first colour.
        assert_eq!(change_effect(&wave, EffectKind::Static), Effect::Static { color: c("#FF0000") });
        // A single colour grows to the two a Wave needs.
        let ripple =
            Effect::Ripple { palette: Palette::Colors(vec![c("#FF0000")]), speed: 50, reach: 12, thickness: 1.5 };
        match change_effect(&ripple, EffectKind::Wave) {
            Effect::Wave { palette: Palette::Colors(cs), .. } => {
                assert_eq!(cs.len(), 2);
                assert_eq!(cs[0], c("#FF0000"));
            }
            e => panic!("{e:?}"),
        }
        // Static's colour becomes a one-colour list.
        let stat = Effect::Static { color: c("#123456") };
        assert_eq!(change_effect(&stat, EffectKind::Reactive).palette(), Some(&Palette::Colors(vec![c("#123456")])));
        // Random does not fit a Wave, so the Wave gets its own default colours.
        let star = Effect::default_for(EffectKind::Starlight);
        assert_eq!(change_effect(&star, EffectKind::Wave), Effect::default_for(EffectKind::Wave));
        // Effects without colours give the plain default.
        assert_eq!(
            change_effect(&Effect::Spectrum { speed: 3 }, EffectKind::Fire),
            Effect::default_for(EffectKind::Fire)
        );
        // Every change gives an effect of the asked kind.
        for from in EffectKind::ALL {
            for to in EffectKind::ALL {
                assert_eq!(change_effect(&Effect::default_for(from), to).kind(), to);
            }
        }
    }

    #[test]
    fn setup_names_follow_the_store_rules() {
        assert!(check_setup_name("Blue night").is_ok());
        assert!(check_setup_name(&"x".repeat(64)).is_ok());
        for bad in ["", "   ", "a/b", ".hidden", "a\0b"] {
            assert!(check_setup_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_setup_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn only_real_changes_need_a_question_before_switching() {
        let default = Setup::owner_default();
        // The untouched default is never lost: it can be loaded again from the menu.
        assert!(!ask_before_switching(&default, None));
        let mut edited = default.clone();
        edited.brightness = 40;
        assert!(ask_before_switching(&edited, None));
        assert!(!ask_before_switching(&edited, Some(&edited.clone())));
        assert!(ask_before_switching(&edited, Some(&default)));
    }

    #[test]
    fn direction_buttons_match_the_wave_angles() {
        let deg = |name| DIRECTIONS.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(deg("Right"), 0);
        assert_eq!(deg("Up"), 90);
        assert_eq!(deg("Left"), 180);
        assert_eq!(deg("Down"), 270);
    }
}
