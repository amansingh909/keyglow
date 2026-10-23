use crate::setup::{MAX_COLS, MAX_ROWS};

/// (Linux key code, row, col) on Razer's standard keyboard grid, ANSI layout. Full-size
/// keyboards use all 22 columns; tenkeyless ones stop before the number pad at column 18.
const KEYS: &[(u16, usize, usize)] = &[
    // row 0
    (1, 0, 1), (59, 0, 3), (60, 0, 4), (61, 0, 5), (62, 0, 6), (63, 0, 7), (64, 0, 8),
    (65, 0, 9), (66, 0, 10), (67, 0, 11), (68, 0, 12), (87, 0, 13), (88, 0, 14),
    (99, 0, 15), (70, 0, 16), (119, 0, 17),
    // row 1
    (41, 1, 1), (2, 1, 2), (3, 1, 3), (4, 1, 4), (5, 1, 5), (6, 1, 6), (7, 1, 7), (8, 1, 8),
    (9, 1, 9), (10, 1, 10), (11, 1, 11), (12, 1, 12), (13, 1, 13), (14, 1, 14),
    (110, 1, 15), (102, 1, 16), (104, 1, 17),
    (69, 1, 18), (98, 1, 19), (55, 1, 20), (74, 1, 21),
    // row 2
    (15, 2, 1), (16, 2, 2), (17, 2, 3), (18, 2, 4), (19, 2, 5), (20, 2, 6), (21, 2, 7),
    (22, 2, 8), (23, 2, 9), (24, 2, 10), (25, 2, 11), (26, 2, 12), (27, 2, 13), (43, 2, 14),
    (111, 2, 15), (107, 2, 16), (109, 2, 17),
    (71, 2, 18), (72, 2, 19), (73, 2, 20), (78, 2, 21),
    // row 3
    (58, 3, 1), (30, 3, 2), (31, 3, 3), (32, 3, 4), (33, 3, 5), (34, 3, 6), (35, 3, 7),
    (36, 3, 8), (37, 3, 9), (38, 3, 10), (39, 3, 11), (40, 3, 12), (28, 3, 14),
    (75, 3, 18), (76, 3, 19), (77, 3, 20),
    // row 4
    (42, 4, 1), (44, 4, 3), (45, 4, 4), (46, 4, 5), (47, 4, 6), (48, 4, 7), (49, 4, 8),
    (50, 4, 9), (51, 4, 10), (52, 4, 11), (53, 4, 12), (54, 4, 14), (103, 4, 16),
    (79, 4, 18), (80, 4, 19), (81, 4, 20), (96, 4, 21),
    // row 5
    (29, 5, 1), (125, 5, 2), (56, 5, 3), (57, 5, 7), (100, 5, 11), (127, 5, 13),
    (97, 5, 14), (105, 5, 15), (108, 5, 16), (106, 5, 17),
    (82, 5, 19), (83, 5, 20),
];

/// The Fn key has an LED but no Linux key code of its own.
const FN_CELL: (usize, usize) = (5, 12);

/// The cell a key sits in on the standard grid. Keyboards with other grids get the same cells,
/// which is also what OpenRazer's own ripple does; the engine drops cells outside the grid.
pub fn grid_of_keycode(code: u16) -> Option<(usize, usize)> {
    KEYS.iter().find(|k| k.0 == code).map(|k| (k.1, k.2))
}

/// Approximate centre of the key in key units (x to the right, y downward).
/// The function row sits half a unit above the rest.
pub fn key_position(row: usize, col: usize) -> (f32, f32) {
    let y = if row == 0 { 0.0 } else { row as f32 + 0.5 };
    (col as f32, y)
}

/// One keyboard model's light grid: its size and the cells that have a light.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub rows: usize,
    pub cols: usize,
    /// Row by row.
    leds: Vec<(usize, usize)>,
}

impl Layout {
    /// A `rows` by `cols` grid. The two sizes of the standard grid (6 by 22 full size, 6 by 18
    /// tenkeyless) light the cells of its keys; any other size counts every cell as lit, since
    /// where its keys sit isn't known.
    pub fn for_size(rows: usize, cols: usize) -> Layout {
        let (rows, cols) = (rows.clamp(1, MAX_ROWS), cols.clamp(1, MAX_COLS));
        let standard = rows == 6 && (cols == 18 || cols == 22);
        let leds = (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .filter(|&(r, c)| !standard || (r, c) == FN_CELL || KEYS.iter().any(|k| (k.1, k.2) == (r, c)))
            .collect();
        Layout { rows, cols, leds }
    }

    /// The BlackWidow V3 Tenkeyless, the keyboard Keyglow was built on.
    pub fn tenkeyless() -> Layout {
        Layout::for_size(6, 18)
    }

    pub fn has_led(&self, row: usize, col: usize) -> bool {
        self.leds.contains(&(row, col))
    }

    pub fn leds(&self) -> &[(usize, usize)] {
        &self.leds
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        row < self.rows && col < self.cols
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_keys_land_on_the_grid() {
        assert_eq!(grid_of_keycode(1), Some((0, 1)));
        assert_eq!(grid_of_keycode(119), Some((0, 17)));
        assert_eq!(grid_of_keycode(57), Some((5, 7)));
        assert_eq!(grid_of_keycode(43), Some((2, 14)));
        assert_eq!(grid_of_keycode(106), Some((5, 17)));
        assert_eq!(grid_of_keycode(500), None);
    }
    #[test]
    fn the_number_pad_sits_right_of_the_arrows() {
        assert_eq!(grid_of_keycode(69), Some((1, 18)), "Num Lock");
        assert_eq!(grid_of_keycode(74), Some((1, 21)), "keypad minus");
        assert_eq!(grid_of_keycode(96), Some((4, 21)), "keypad Enter");
        assert_eq!(grid_of_keycode(82), Some((5, 19)), "keypad 0");
        assert_eq!(grid_of_keycode(83), Some((5, 20)), "keypad dot");
    }
    #[test]
    fn column_zero_has_no_leds() {
        let tkl = Layout::tenkeyless();
        for r in 0..tkl.rows {
            assert!(!tkl.has_led(r, 0));
        }
    }
    #[test]
    fn tenkeyless_leds_are_its_keys_plus_fn() {
        let tkl = Layout::tenkeyless();
        assert!(tkl.has_led(5, 12));
        assert!(!tkl.has_led(5, 4));
        assert!(!tkl.has_led(3, 13));
        assert!(!tkl.has_led(6, 1));
        assert!(!tkl.has_led(0, 18));
        assert!(!tkl.has_led(1, 18), "no number pad");
        assert_eq!(tkl.leds().len(), 16 + 17 + 17 + 13 + 13 + 10 + 1);
    }
    #[test]
    fn full_size_adds_the_number_pad() {
        let full = Layout::for_size(6, 22);
        assert!(full.has_led(1, 18) && full.has_led(4, 21) && full.has_led(5, 20));
        assert!(!full.has_led(5, 18) && !full.has_led(3, 21), "no keys there");
        assert_eq!(full.leds().len(), Layout::tenkeyless().leds().len() + 17);
    }
    #[test]
    fn other_sizes_light_every_cell() {
        let mini = Layout::for_size(5, 15);
        assert_eq!((mini.rows, mini.cols), (5, 15));
        assert_eq!(mini.leds().len(), 75);
        assert!(mini.has_led(0, 0) && mini.has_led(4, 14) && !mini.has_led(5, 0));
        assert_eq!(Layout::for_size(1, 10).leds().len(), 10);
        // Sizes past the largest grid are cut down to it.
        let huge = Layout::for_size(40, 40);
        assert_eq!((huge.rows, huge.cols), (MAX_ROWS, MAX_COLS));
        assert_eq!(Layout::for_size(0, 0).leds(), &[(0, 0)]);
    }
    #[test]
    fn every_mapped_key_is_a_distinct_full_size_led() {
        let full = Layout::for_size(6, 22);
        let mut seen = std::collections::HashSet::new();
        for code in 0..=255u16 {
            if let Some((r, c)) = grid_of_keycode(code) {
                assert!(full.has_led(r, c), "code {code}");
                assert!(seen.insert((r, c)), "dup cell for code {code}");
            }
        }
        assert_eq!(seen.len(), full.leds().len() - 1, "every LED but Fn has a key");
    }
    #[test]
    fn key_position_is_monotonic_in_row_and_col() {
        let (x1, y1) = key_position(0, 1);
        let (x2, y2) = key_position(5, 17);
        assert!(x2 > x1 && y2 > y1);
    }
}
