use crate::color::Rgb;
use crate::setup::{COLS, ROWS};

pub type Frame = [[Rgb; COLS]; ROWS];

pub const BLACK_FRAME: Frame = [[Rgb::BLACK; COLS]; ROWS];

/// Custom-frame bytes: for each row, `[row, 0, COLS - 1]` then COLS RGB triples.
pub fn encode(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::with_capacity(ROWS * (3 + COLS * 3));
    for (r, row) in frame.iter().enumerate() {
        out.extend_from_slice(&[r as u8, 0, (COLS - 1) as u8]);
        for c in row {
            out.extend_from_slice(&[c.r, c.g, c.b]);
        }
    }
    out
}

/// Scales every channel by brightness percent (values above 100 count as 100).
pub fn scale(frame: &Frame, brightness: u8) -> Frame {
    let b = brightness.min(100) as u32;
    let m = |x: u8| ((x as u32 * b + 50) / 100) as u8;
    let mut out = *frame;
    for row in out.iter_mut() {
        for c in row.iter_mut() {
            *c = Rgb { r: m(c.r), g: m(c.g), b: m(c.b) };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encode_writes_six_rows_of_eighteen() {
        let mut f = BLACK_FRAME;
        f[2][5] = Rgb { r: 1, g: 2, b: 3 };
        let b = encode(&f);
        assert_eq!(b.len(), 342);
        assert_eq!(&b[114..117], &[2, 0, 17]);
        assert_eq!(&b[117 + 15..117 + 18], &[1, 2, 3]);
    }
    #[test]
    fn scale_halves_at_fifty() {
        let mut f = BLACK_FRAME;
        f[1][1] = Rgb { r: 200, g: 100, b: 0 };
        let s = scale(&f, 50);
        assert_eq!(s[1][1], Rgb { r: 100, g: 50, b: 0 });
        assert_eq!(scale(&f, 100)[1][1], f[1][1]);
        assert_eq!(scale(&f, 0)[1][1], Rgb::BLACK);
    }
}
