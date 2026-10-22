use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const WHITE: Rgb = Rgb { r: 255, g: 255, b: 255 };
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    pub fn from_hex(s: &str) -> Option<Rgb> {
        let h = s.strip_prefix('#')?;
        if h.len() != 6 || !h.is_ascii() {
            return None;
        }
        let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        Some(Rgb { r: p(0)?, g: p(2)?, b: p(4)? })
    }

    pub fn hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Rgb, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::from_hex(&s).ok_or_else(|| serde::de::Error::custom(format!("bad colour {s:?}, expected #RRGGBB")))
    }
}

fn byte(x: f32) -> u8 {
    (x * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// h wraps around 0..1; s and v are clamped to 0..1.
pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgb { r: byte(r), g: byte(g), b: byte(b) }
}

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t + 0.5) as u8;
    Rgb { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b) }
}

/// Cyclic gradient: t wraps through every colour and back to the first.
pub fn gradient(colors: &[Rgb], t: f32) -> Rgb {
    match colors.len() {
        0 => Rgb::BLACK,
        1 => colors[0],
        n => {
            let pos = t.rem_euclid(1.0) * n as f32;
            let i = pos.floor();
            let frac = pos - i;
            let i = i as usize % n;
            lerp(colors[i], colors[(i + 1) % n], frac)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_round_trips() {
        let c = Rgb::from_hex("#03fffa").unwrap();
        assert_eq!(c, Rgb { r: 3, g: 255, b: 250 });
        assert_eq!(c.hex(), "#03FFFA");
    }
    #[test]
    fn gradient_cycles_back_to_the_first_colour() {
        let cs = [Rgb::BLACK, Rgb::WHITE];
        assert_eq!(gradient(&cs, 0.0), Rgb::BLACK);
        assert_eq!(gradient(&cs, 0.5), Rgb::WHITE);
        assert_eq!(gradient(&cs, 1.0), Rgb::BLACK);
    }
    #[test]
    fn serde_uses_hex_strings() {
        let j = serde_json::to_string(&Rgb { r: 255, g: 0, b: 127 }).unwrap();
        assert_eq!(j, "\"#FF007F\"");
        assert_eq!(serde_json::from_str::<Rgb>("\"#ff007f\"").unwrap(), Rgb { r: 255, g: 0, b: 127 });
        assert!(serde_json::from_str::<Rgb>("\"red\"").is_err());
    }
    #[test]
    fn hsv_primaries_and_wrap() {
        assert_eq!(hsv(0.0, 1.0, 1.0), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv(1.0, 1.0, 1.0), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv(1.0 / 3.0, 1.0, 1.0), Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsv(0.0, 0.0, 1.0), Rgb::WHITE);
    }
    #[test]
    fn single_colour_gradient_is_that_colour() {
        let c = Rgb { r: 9, g: 8, b: 7 };
        assert_eq!(gradient(&[c], 0.37), c);
    }
}
