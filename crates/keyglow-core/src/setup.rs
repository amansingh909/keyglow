use crate::color::Rgb;
use serde::{Deserialize, Serialize};

pub const ROWS: usize = 6;
pub const COLS: usize = 18;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub name: String,
    pub fps: u32,
    pub brightness: u8,
    /// Bottom layer first.
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub enabled: bool,
    pub opacity: u8,
    pub effect: Effect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    Colors(Vec<Rgb>),
    Random,
    Rainbow,
}

impl Palette {
    fn of(hexes: &[&str]) -> Palette {
        Palette::Colors(hexes.iter().filter_map(|h| Rgb::from_hex(h)).collect())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    Static { color: Rgb },
    Breathing { palette: Palette, speed: u8 },
    Spectrum { speed: u8 },
    /// direction in degrees: 0 left to right, 90 bottom to top, 180 right to left, 270 top to bottom.
    /// band is the width of one colour band in keys.
    Wave { palette: Palette, speed: u8, direction: u16, band: u8 },
    Wheel { palette: Palette, speed: u8, clockwise: bool },
    Starlight { palette: Palette, density: u8, duration_ms: u32 },
    Reactive { palette: Palette, duration_ms: u32 },
    Ripple { palette: Palette, speed: u8, reach: u8, thickness: f32 },
    Fire { palette: Palette, speed: u8, height: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Static,
    Breathing,
    Spectrum,
    Wave,
    Wheel,
    Starlight,
    Reactive,
    Ripple,
    Fire,
}

impl EffectKind {
    pub const ALL: [EffectKind; 9] = [
        EffectKind::Static,
        EffectKind::Breathing,
        EffectKind::Spectrum,
        EffectKind::Wave,
        EffectKind::Wheel,
        EffectKind::Starlight,
        EffectKind::Reactive,
        EffectKind::Ripple,
        EffectKind::Fire,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EffectKind::Static => "Static",
            EffectKind::Breathing => "Breathing",
            EffectKind::Spectrum => "Spectrum",
            EffectKind::Wave => "Wave",
            EffectKind::Wheel => "Wheel",
            EffectKind::Starlight => "Starlight",
            EffectKind::Reactive => "Reactive",
            EffectKind::Ripple => "Ripple",
            EffectKind::Fire => "Fire",
        }
    }
}

impl Effect {
    pub fn kind(&self) -> EffectKind {
        match self {
            Effect::Static { .. } => EffectKind::Static,
            Effect::Breathing { .. } => EffectKind::Breathing,
            Effect::Spectrum { .. } => EffectKind::Spectrum,
            Effect::Wave { .. } => EffectKind::Wave,
            Effect::Wheel { .. } => EffectKind::Wheel,
            Effect::Starlight { .. } => EffectKind::Starlight,
            Effect::Reactive { .. } => EffectKind::Reactive,
            Effect::Ripple { .. } => EffectKind::Ripple,
            Effect::Fire { .. } => EffectKind::Fire,
        }
    }

    pub fn palette(&self) -> Option<&Palette> {
        match self {
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

    pub fn default_for(kind: EffectKind) -> Effect {
        match kind {
            EffectKind::Static => Effect::Static { color: Rgb::WHITE },
            EffectKind::Breathing => Effect::Breathing { palette: Palette::of(&["#03FFFA"]), speed: 30 },
            EffectKind::Spectrum => Effect::Spectrum { speed: 30 },
            EffectKind::Wave => Effect::Wave {
                palette: Palette::of(&["#03FFFA", "#55007F"]),
                speed: 40,
                direction: 0,
                band: 18,
            },
            EffectKind::Wheel => Effect::Wheel { palette: Palette::Rainbow, speed: 30, clockwise: true },
            EffectKind::Starlight => Effect::Starlight { palette: Palette::Random, density: 30, duration_ms: 1200 },
            EffectKind::Reactive => Effect::Reactive { palette: Palette::of(&["#FFFFFF"]), duration_ms: 800 },
            EffectKind::Ripple => Effect::Ripple { palette: Palette::of(&["#FFFFFF"]), speed: 50, reach: 12, thickness: 1.5 },
            EffectKind::Fire => Effect::Fire {
                palette: Palette::of(&["#FF0000", "#FF7F00", "#FFFF00"]),
                speed: 50,
                height: 60,
            },
        }
    }

    fn sanitized(self) -> Effect {
        let kind = self.kind();
        let fix_palette = |p: Palette| match p {
            Palette::Colors(colors) if colors.is_empty() => {
                Effect::default_for(kind).palette().cloned().unwrap_or(Palette::Rainbow)
            }
            other => other,
        };
        let sp = |s: u8| s.clamp(1, 100);
        let dur = |d: u32| d.clamp(100, 10_000);
        match self {
            Effect::Static { color } => Effect::Static { color },
            Effect::Breathing { palette, speed } => Effect::Breathing { palette: fix_palette(palette), speed: sp(speed) },
            Effect::Spectrum { speed } => Effect::Spectrum { speed: sp(speed) },
            Effect::Wave { palette, speed, direction, band } => {
                Effect::Wave { palette: fix_palette(palette), speed: sp(speed), direction, band: band.clamp(2, 36) }
            }
            Effect::Wheel { palette, speed, clockwise } => {
                Effect::Wheel { palette: fix_palette(palette), speed: sp(speed), clockwise }
            }
            Effect::Starlight { palette, density, duration_ms } => {
                Effect::Starlight { palette: fix_palette(palette), density: sp(density), duration_ms: dur(duration_ms) }
            }
            Effect::Reactive { palette, duration_ms } => {
                Effect::Reactive { palette: fix_palette(palette), duration_ms: dur(duration_ms) }
            }
            Effect::Ripple { palette, speed, reach, thickness } => Effect::Ripple {
                palette: fix_palette(palette),
                speed: sp(speed),
                reach: reach.clamp(1, 36),
                thickness: if thickness.is_nan() { 1.5 } else { thickness.clamp(0.5, 6.0) },
            },
            Effect::Fire { palette, speed, height } => {
                Effect::Fire { palette: fix_palette(palette), speed: sp(speed), height: sp(height) }
            }
        }
    }
}

impl Setup {
    pub fn owner_default() -> Setup {
        let layer = |kind| Layer { enabled: true, opacity: 100, effect: Effect::default_for(kind) };
        Setup {
            name: "Default".into(),
            fps: 30,
            brightness: 100,
            layers: vec![layer(EffectKind::Wave), layer(EffectKind::Starlight), layer(EffectKind::Ripple)],
        }
    }

    /// Clamps every value to its allowed range (for hand-edited files).
    pub fn sanitized(mut self) -> Setup {
        self.fps = self.fps.clamp(10, 60);
        self.brightness = self.brightness.min(100);
        self.layers = self
            .layers
            .into_iter()
            .map(|l| Layer { enabled: l.enabled, opacity: l.opacity.min(100), effect: l.effect.sanitized() })
            .collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_default_round_trips_json() {
        let s = Setup::owner_default();
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Setup>(&j).unwrap(), s);
        assert_eq!(s.layers.len(), 3);
    }
    #[test]
    fn owner_default_order_is_wave_starlight_ripple() {
        let s = Setup::owner_default();
        let kinds: Vec<_> = s.layers.iter().map(|l| l.effect.kind()).collect();
        assert_eq!(kinds, [EffectKind::Wave, EffectKind::Starlight, EffectKind::Ripple]);
        assert_eq!(s.name, "Default");
        assert_eq!(s.fps, 30);
    }
    #[test]
    fn sanitized_clamps_hand_edited_values() {
        let mut s = Setup::owner_default();
        s.fps = 0;
        assert_eq!(s.clone().sanitized().fps, 10);
        s.fps = 500;
        s.brightness = 250;
        s.layers[0].opacity = 255;
        s.layers[0].effect = Effect::Wave { palette: Palette::Colors(vec![]), speed: 0, direction: 90, band: 200 };
        let s = s.sanitized();
        assert_eq!(s.fps, 60);
        assert_eq!(s.brightness, 100);
        assert_eq!(s.layers[0].opacity, 100);
        match &s.layers[0].effect {
            Effect::Wave { palette, speed, band, .. } => {
                assert_eq!(*speed, 1);
                assert_eq!(*band, 36);
                assert_eq!(palette, &Effect::default_for(EffectKind::Wave).palette().unwrap().clone());
            }
            _ => panic!(),
        }
    }
    #[test]
    fn effects_serialize_with_snake_case_type_tag() {
        let j = serde_json::to_string(&Effect::default_for(EffectKind::Static)).unwrap();
        assert!(j.contains("\"type\":\"static\""));
        let p: Palette = serde_json::from_str("\"random\"").unwrap();
        assert_eq!(p, Palette::Random);
        let p: Palette = serde_json::from_str("{\"colors\":[\"#FF0000\"]}").unwrap();
        assert_eq!(p, Palette::Colors(vec![Rgb { r: 255, g: 0, b: 0 }]));
    }
    #[test]
    fn every_kind_has_a_default_of_that_kind_and_a_label() {
        for k in EffectKind::ALL {
            assert_eq!(Effect::default_for(k).kind(), k);
            assert!(!k.label().is_empty());
        }
        assert_eq!(EffectKind::ALL.len(), 9);
    }
    #[test]
    fn fire_defaults_to_red_orange_yellow() {
        match Effect::default_for(EffectKind::Fire) {
            Effect::Fire { palette: Palette::Colors(colors), .. } => {
                assert_eq!(colors, vec![Rgb::from_hex("#FF0000").unwrap(), Rgb::from_hex("#FF7F00").unwrap(), Rgb::from_hex("#FFFF00").unwrap()]);
            }
            _ => panic!(),
        }
    }
}
