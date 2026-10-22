use crate::setup::{COLS, ROWS};

/// (Linux key code, row, col), ANSI layout.
const KEYS: &[(u16, usize, usize)] = &[
    // row 0
    (1, 0, 1), (59, 0, 3), (60, 0, 4), (61, 0, 5), (62, 0, 6), (63, 0, 7), (64, 0, 8),
    (65, 0, 9), (66, 0, 10), (67, 0, 11), (68, 0, 12), (87, 0, 13), (88, 0, 14),
    (99, 0, 15), (70, 0, 16), (119, 0, 17),
    // row 1
    (41, 1, 1), (2, 1, 2), (3, 1, 3), (4, 1, 4), (5, 1, 5), (6, 1, 6), (7, 1, 7), (8, 1, 8),
    (9, 1, 9), (10, 1, 10), (11, 1, 11), (12, 1, 12), (13, 1, 13), (14, 1, 14),
    (110, 1, 15), (102, 1, 16), (104, 1, 17),
    // row 2
    (15, 2, 1), (16, 2, 2), (17, 2, 3), (18, 2, 4), (19, 2, 5), (20, 2, 6), (21, 2, 7),
    (22, 2, 8), (23, 2, 9), (24, 2, 10), (25, 2, 11), (26, 2, 12), (27, 2, 13), (43, 2, 14),
    (111, 2, 15), (107, 2, 16), (109, 2, 17),
    // row 3
    (58, 3, 1), (30, 3, 2), (31, 3, 3), (32, 3, 4), (33, 3, 5), (34, 3, 6), (35, 3, 7),
    (36, 3, 8), (37, 3, 9), (38, 3, 10), (39, 3, 11), (40, 3, 12), (28, 3, 14),
    // row 4
    (42, 4, 1), (44, 4, 3), (45, 4, 4), (46, 4, 5), (47, 4, 6), (48, 4, 7), (49, 4, 8),
    (50, 4, 9), (51, 4, 10), (52, 4, 11), (53, 4, 12), (54, 4, 14), (103, 4, 16),
    // row 5
    (29, 5, 1), (125, 5, 2), (56, 5, 3), (57, 5, 7), (100, 5, 11), (127, 5, 13),
    (97, 5, 14), (105, 5, 15), (108, 5, 16), (106, 5, 17),
];

/// The Fn key has an LED but no Linux key code of its own.
const FN_CELL: (usize, usize) = (5, 12);

pub fn grid_of_keycode(code: u16) -> Option<(usize, usize)> {
    KEYS.iter().find(|k| k.0 == code).map(|k| (k.1, k.2))
}

pub fn has_led(row: usize, col: usize) -> bool {
    if row >= ROWS || col >= COLS {
        return false;
    }
    (row, col) == FN_CELL || KEYS.iter().any(|k| k.1 == row && k.2 == col)
}

/// Approximate centre of the key in key units (x to the right, y downward).
/// The function row sits half a unit above the rest.
pub fn key_position(row: usize, col: usize) -> (f32, f32) {
    let y = if row == 0 { 0.0 } else { row as f32 + 0.5 };
    (col as f32, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::ROWS;
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
    fn column_zero_has_no_leds() {
        for r in 0..ROWS {
            assert!(!has_led(r, 0));
        }
    }
    #[test]
    fn has_led_covers_table_plus_fn() {
        assert!(has_led(5, 12));
        assert!(!has_led(5, 4));
        assert!(!has_led(3, 13));
        assert!(!has_led(6, 1));
        assert!(!has_led(0, 18));
    }
    #[test]
    fn every_mapped_key_has_a_led_and_distinct_cell() {
        let mut seen = std::collections::HashSet::new();
        for code in 0..=255u16 {
            if let Some((r, c)) = grid_of_keycode(code) {
                assert!(has_led(r, c), "code {code}");
                assert!(seen.insert((r, c)), "dup cell for code {code}");
            }
        }
        assert_eq!(seen.len(), 16 + 17 + 17 + 13 + 13 + 10);
    }
    #[test]
    fn key_position_is_monotonic_in_row_and_col() {
        let (x1, y1) = key_position(0, 1);
        let (x2, y2) = key_position(5, 17);
        assert!(x2 > x1 && y2 > y1);
    }
}
