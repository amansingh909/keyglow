use crate::color::Rgb;
use crate::setup::{MAX_COLS, MAX_ROWS};

pub type Frame = [[Rgb; MAX_COLS]; MAX_ROWS];

pub const BLACK_FRAME: Frame = [[Rgb::BLACK; MAX_COLS]; MAX_ROWS];

/// Custom-frame bytes for a keyboard with a `rows` by `cols` grid: for each row,
/// `[row, 0, cols - 1]` then `cols` RGB triples.
pub fn encode(frame: &Frame, rows: usize, cols: usize) -> Vec<u8> {
    let (rows, cols) = (rows.clamp(1, MAX_ROWS), cols.clamp(1, MAX_COLS));
    let mut out = Vec::with_capacity(rows * (3 + cols * 3));
    for (r, row) in frame.iter().take(rows).enumerate() {
        out.extend_from_slice(&[r as u8, 0, (cols - 1) as u8]);
        for c in &row[..cols] {
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
        f[2][18] = Rgb::WHITE;
        let b = encode(&f, 6, 18);
        assert_eq!(b.len(), 342);
        assert_eq!(&b[114..117], &[2, 0, 17]);
        assert_eq!(&b[117 + 15..117 + 18], &[1, 2, 3]);
        assert!(!b.contains(&255), "cells past the grid are left out");
    }
    #[test]
    fn encode_follows_the_keyboard_size() {
        let mut f = BLACK_FRAME;
        f[1][21] = Rgb { r: 9, g: 8, b: 7 };
        let full = encode(&f, 6, 22);
        assert_eq!(full.len(), 6 * (3 + 22 * 3));
        assert_eq!(&full[69..72], &[1, 0, 21]);
        assert_eq!(&full[69 + 3 + 21 * 3..][..3], &[9, 8, 7]);
        assert_eq!(encode(&f, 1, 10).len(), 3 + 30);
        assert_eq!(encode(&f, 99, 99).len(), MAX_ROWS * (3 + MAX_COLS * 3));
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
