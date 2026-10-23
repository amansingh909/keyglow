use crate::color::Rgb;
use serde::{Deserialize, Serialize};

/// The largest light grid of any supported keyboard (see `keyboards`). Frames are this big, and
/// each keyboard uses its own corner of them.
pub const MAX_ROWS: usize = 9;
pub const MAX_COLS: usize = 25;

fn num<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    f64::deserialize(d)
}
fn clamp_u8<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    Ok(num(d)?.clamp(0.0, u8::MAX as f64).round() as u8)
}
fn clamp_u16<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    Ok(num(d)?.clamp(0.0, u16::MAX as f64).round() as u16)
}
fn clamp_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    Ok(num(d)?.clamp(0.0, u32::MAX as f64).round() as u32)
}
fn clamp_f32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    Ok(num(d)? as f32)
}
fn d_name() -> String {
    "Default".into()
}
fn d_fps() -> u32 {
    30
}
fn d_100() -> u8 {
    100
}
fn d_true() -> bool {
    true
}
fn d_white() -> Rgb {
    Rgb::WHITE
}
fn d_pal(kind: EffectKind) -> Palette {
    Effect::default_for(kind).palette().cloned().unwrap_or(Palette::Rainbow)
}
fn d_pal_breathing() -> Palette { d_pal(EffectKind::Breathing) }
fn d_pal_wave() -> Palette { d_pal(EffectKind::Wave) }
fn d_pal_wheel() -> Palette { d_pal(EffectKind::Wheel) }
fn d_pal_starlight() -> Palette { d_pal(EffectKind::Starlight) }
fn d_pal_reactive() -> Palette { d_pal(EffectKind::Reactive) }
fn d_pal_ripple() -> Palette { d_pal(EffectKind::Ripple) }
fn d_pal_fire() -> Palette { d_pal(EffectKind::Fire) }
fn d_30() -> u8 { 30 }
fn d_40() -> u8 { 40 }
fn d_50() -> u8 { 50 }
fn d_60() -> u8 { 60 }
fn d_18() -> u8 { 18 }
fn d_12() -> u8 { 12 }
fn d_0_u16() -> u16 { 0 }
fn d_1200() -> u32 { 1200 }
fn d_800() -> u32 { 800 }
fn d_1_5() -> f32 { 1.5 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    #[serde(default = "d_name")]
    pub name: String,
    #[serde(default = "d_fps", deserialize_with = "clamp_u32")]
    pub fps: u32,
    #[serde(default = "d_100", deserialize_with = "clamp_u8")]
    pub brightness: u8,
    /// Bottom layer first.
    #[serde(default)]
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    #[serde(default = "d_true")]
    pub enabled: bool,
    #[serde(default = "d_100", deserialize_with = "clamp_u8")]
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
    Static {
        #[serde(default = "d_white")]
        color: Rgb,
    },
    Breathing {
        #[serde(default = "d_pal_breathing")]
        palette: Palette,
        #[serde(default = "d_30", deserialize_with = "clamp_u8")]
        speed: u8,
    },
    Spectrum {
        #[serde(default = "d_30", deserialize_with = "clamp_u8")]
        speed: u8,
    },
    /// direction in degrees: 0 left to right, 90 bottom to top, 180 right to left, 270 top to bottom.
    /// band is the width of one colour band in keys.
    Wave {
        #[serde(default = "d_pal_wave")]
        palette: Palette,
        #[serde(default = "d_40", deserialize_with = "clamp_u8")]
        speed: u8,
        #[serde(default = "d_0_u16", deserialize_with = "clamp_u16")]
        direction: u16,
        #[serde(default = "d_18", deserialize_with = "clamp_u8")]
        band: u8,
    },
    Wheel {
        #[serde(default = "d_pal_wheel")]
        palette: Palette,
        #[serde(default = "d_30", deserialize_with = "clamp_u8")]
        speed: u8,
        #[serde(default = "d_true")]
        clockwise: bool,
    },
    Starlight {
        #[serde(default = "d_pal_starlight")]
        palette: Palette,
        #[serde(default = "d_30", deserialize_with = "clamp_u8")]
        density: u8,
        #[serde(default = "d_1200", deserialize_with = "clamp_u32")]
        duration_ms: u32,
    },
    Reactive {
        #[serde(default = "d_pal_reactive")]
        palette: Palette,
        #[serde(default = "d_800", deserialize_with = "clamp_u32")]
        duration_ms: u32,
    },
    Ripple {
        #[serde(default = "d_pal_ripple")]
        palette: Palette,
        #[serde(default = "d_50", deserialize_with = "clamp_u8")]
        speed: u8,
        #[serde(default = "d_12", deserialize_with = "clamp_u8")]
        reach: u8,
        #[serde(default = "d_1_5", deserialize_with = "clamp_f32")]
        thickness: f32,
    },
    Fire {
        #[serde(default = "d_pal_fire")]
        palette: Palette,
        #[serde(default = "d_50", deserialize_with = "clamp_u8")]
        speed: u8,
        #[serde(default = "d_60", deserialize_with = "clamp_u8")]
        height: u8,
    },
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
    #[test]
    fn missing_fields_get_defaults() {
        let s: Setup = serde_json::from_str(r##"{"layers":[{"effect":{"type":"static","color":"#102030"}}]}"##).unwrap();
        assert_eq!(s.name, "Default");
        assert_eq!(s.fps, 30);
        assert_eq!(s.brightness, 100);
        assert_eq!(s.layers.len(), 1);
        assert!(s.layers[0].enabled);
        assert_eq!(s.layers[0].opacity, 100);
        for k in EffectKind::ALL {
            if k == EffectKind::Static {
                continue;
            }
            let tag = serde_json::to_value(Effect::default_for(k)).unwrap()["type"].as_str().unwrap().to_string();
            let e: Effect = serde_json::from_str(&format!("{{\"type\":\"{tag}\"}}")).unwrap();
            assert_eq!(e, Effect::default_for(k));
        }
        let s: Setup = serde_json::from_str("{}").unwrap();
        assert!(s.layers.is_empty());
    }
    #[test]
    fn oversized_numbers_clamp_instead_of_failing() {
        let j = r#"{"fps":5000,"brightness":-4,"layers":[{"opacity":300,"effect":{"type":"spectrum","speed":999}}]}"#;
        let s: Setup = serde_json::from_str(j).unwrap();
        assert_eq!(s.layers[0].opacity, 255);
        assert_eq!(s.brightness, 0);
        assert_eq!(s.fps, 5000);
        let s = s.sanitized();
        assert_eq!(s.layers[0].opacity, 100);
        assert_eq!(s.fps, 60);
        assert_eq!(s.brightness, 0);
        assert_eq!(s.layers[0].effect, Effect::Spectrum { speed: 100 });
    }
    #[test]
    fn wrong_shapes_are_still_errors() {
        assert!(serde_json::from_str::<Setup>(r#"{"layers":[{"effect":{"type":"disco"}}]}"#).is_err());
        assert!(serde_json::from_str::<Setup>(r#"{"layers":[{"effect":{"speed":5}}]}"#).is_err());
        assert!(serde_json::from_str::<Setup>(r#"{"fps":"fast"}"#).is_err());
        assert!(serde_json::from_str::<Setup>(r#"{"layers":[{"opacity":"high","effect":{"type":"spectrum"}}]}"#).is_err());
        assert!(serde_json::from_str::<Setup>(r#"{"layers":[{"effect":{"type":"spectrum","speed":"x"}}]}"#).is_err());
    }
}
