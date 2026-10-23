//! Every keyboard the OpenRazer `razerkbd` driver can draw custom frames on, and the size of its
//! light grid. Taken from the OpenRazer 3.12.4 driver (which models get `matrix_custom_frame`,
//! and their names) and daemon (their grid sizes). Keyglow has been tested on one of them.

use crate::keymap::Layout;

/// One keyboard model, by its USB product ID (the vendor is always Razer, 1532).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    pub rows: u8,
    pub cols: u8,
}

/// The BlackWidow V3 Tenkeyless, the model Keyglow is tested on.
pub const TESTED: u16 = 0x0A24;

/// Sorted by name.
pub const MODELS: &[Model] = &[
    Model { pid: 0x0241, name: "Razer BlackWidow 2019", rows: 6, cols: 22 },
    Model { pid: 0x0203, name: "Razer BlackWidow Chroma", rows: 6, cols: 22 },
    Model { pid: 0x0211, name: "Razer BlackWidow Chroma (Overwatch)", rows: 6, cols: 22 },
    Model { pid: 0x0209, name: "Razer BlackWidow Chroma Tournament Edition", rows: 6, cols: 22 },
    Model { pid: 0x0221, name: "Razer BlackWidow Chroma V2", rows: 6, cols: 22 },
    Model { pid: 0x0228, name: "Razer BlackWidow Elite", rows: 6, cols: 22 },
    Model { pid: 0x0237, name: "Razer BlackWidow Essential", rows: 6, cols: 22 },
    Model { pid: 0x0214, name: "Razer BlackWidow Ultimate 2016", rows: 6, cols: 22 },
    Model { pid: 0x024E, name: "Razer BlackWidow V3", rows: 6, cols: 22 },
    Model { pid: 0x0258, name: "Razer BlackWidow V3 Mini HyperSpeed (Wired)", rows: 5, cols: 16 },
    Model { pid: 0x0271, name: "Razer BlackWidow V3 Mini HyperSpeed (Wireless)", rows: 5, cols: 16 },
    Model { pid: 0x025A, name: "Razer BlackWidow V3 Pro (Wired)", rows: 6, cols: 22 },
    Model { pid: 0x025C, name: "Razer BlackWidow V3 Pro (Wireless)", rows: 6, cols: 22 },
    Model { pid: 0x0A24, name: "Razer BlackWidow V3 Tenkeyless", rows: 6, cols: 18 },
    Model { pid: 0x0287, name: "Razer BlackWidow V4", rows: 8, cols: 23 },
    Model { pid: 0x02A5, name: "Razer BlackWidow V4 75%", rows: 6, cols: 16 },
    Model { pid: 0x02B9, name: "Razer BlackWidow V4 Mini Hyperspeed (Wired)", rows: 5, cols: 14 },
    Model { pid: 0x02BA, name: "Razer BlackWidow V4 Mini Hyperspeed (Wireless)", rows: 5, cols: 14 },
    Model { pid: 0x028D, name: "Razer BlackWidow V4 Pro", rows: 8, cols: 23 },
    Model { pid: 0x02D7, name: "Razer BlackWidow V4 Tenkeyless HyperSpeed (Wired)", rows: 6, cols: 18 },
    Model { pid: 0x02D5, name: "Razer BlackWidow V4 Tenkeyless HyperSpeed (Wireless)", rows: 6, cols: 18 },
    Model { pid: 0x0293, name: "Razer BlackWidow V4 X", rows: 6, cols: 22 },
    Model { pid: 0x0216, name: "Razer BlackWidow X Chroma", rows: 6, cols: 22 },
    Model { pid: 0x021A, name: "Razer BlackWidow X Chroma Tournament Edition", rows: 6, cols: 22 },
    Model { pid: 0x0217, name: "Razer BlackWidow X Ultimate", rows: 6, cols: 22 },
    Model { pid: 0x0224, name: "Razer Blade (Late 2016)", rows: 6, cols: 16 },
    Model { pid: 0x0270, name: "Razer Blade 14 (2021)", rows: 6, cols: 16 },
    Model { pid: 0x028C, name: "Razer Blade 14 (2022)", rows: 6, cols: 16 },
    Model { pid: 0x029D, name: "Razer Blade 14 (2023)", rows: 6, cols: 16 },
    Model { pid: 0x02B6, name: "Razer Blade 14 (2024)", rows: 6, cols: 16 },
    Model { pid: 0x02C5, name: "Razer Blade 14 (2025)", rows: 6, cols: 16 },
    Model { pid: 0x0233, name: "Razer Blade 15 (2018)", rows: 6, cols: 16 },
    Model { pid: 0x0240, name: "Razer Blade 15 (2018) Mercury", rows: 6, cols: 16 },
    Model { pid: 0x023A, name: "Razer Blade 15 (2019) Advanced", rows: 6, cols: 16 },
    Model { pid: 0x029E, name: "Razer Blade 15 (2023)", rows: 6, cols: 16 },
    Model { pid: 0x0245, name: "Razer Blade 15 (Mid 2019) Mercury White", rows: 6, cols: 16 },
    Model { pid: 0x0253, name: "Razer Blade 15 Advanced (2020)", rows: 6, cols: 16 },
    Model { pid: 0x026D, name: "Razer Blade 15 Advanced (Early 2021)", rows: 6, cols: 16 },
    Model { pid: 0x028A, name: "Razer Blade 15 Advanced (Early 2022)", rows: 6, cols: 16 },
    Model { pid: 0x0276, name: "Razer Blade 15 Advanced (Mid 2021)", rows: 6, cols: 16 },
    Model { pid: 0x024D, name: "Razer Blade 15 Studio Edition (2019)", rows: 6, cols: 16 },
    Model { pid: 0x029F, name: "Razer Blade 16 (2023)", rows: 6, cols: 16 },
    Model { pid: 0x02C6, name: "Razer Blade 16 (2025)", rows: 6, cols: 17 },
    Model { pid: 0x028B, name: "Razer Blade 17 (2022)", rows: 6, cols: 16 },
    Model { pid: 0x026E, name: "Razer Blade 17 Pro (Early 2021)", rows: 6, cols: 16 },
    Model { pid: 0x0279, name: "Razer Blade 17 Pro (Mid 2021)", rows: 6, cols: 16 },
    Model { pid: 0x02A0, name: "Razer Blade 18 (2023)", rows: 6, cols: 16 },
    Model { pid: 0x02B8, name: "Razer Blade 18 (2024)", rows: 6, cols: 16 },
    Model { pid: 0x02C7, name: "Razer Blade 18 (2025)", rows: 6, cols: 19 },
    Model { pid: 0x024B, name: "Razer Blade Advanced (Late 2019)", rows: 6, cols: 16 },
    Model { pid: 0x0225, name: "Razer Blade Pro (2017)", rows: 6, cols: 25 },
    Model { pid: 0x0234, name: "Razer Blade Pro (2019)", rows: 6, cols: 16 },
    Model { pid: 0x0256, name: "Razer Blade Pro (Early 2020)", rows: 6, cols: 16 },
    Model { pid: 0x0210, name: "Razer Blade Pro (Late 2016)", rows: 6, cols: 22 },
    Model { pid: 0x024C, name: "Razer Blade Pro (Late 2019)", rows: 6, cols: 16 },
    Model { pid: 0x022F, name: "Razer Blade Pro FullHD (2017)", rows: 6, cols: 25 },
    Model { pid: 0x0205, name: "Razer Blade Stealth", rows: 6, cols: 22 },
    Model { pid: 0x0220, name: "Razer Blade Stealth (Late 2016)", rows: 6, cols: 16 },
    Model { pid: 0x0232, name: "Razer Blade Stealth (Late 2017)", rows: 6, cols: 16 },
    Model { pid: 0x022D, name: "Razer Blade Stealth (Mid 2017)", rows: 6, cols: 16 },
    Model { pid: 0x020F, name: "Razer Blade Stealth (QHD)", rows: 6, cols: 16 },
    Model { pid: 0x022A, name: "Razer Cynosa Chroma", rows: 6, cols: 22 },
    Model { pid: 0x022C, name: "Razer Cynosa Chroma Pro", rows: 6, cols: 22 },
    Model { pid: 0x025E, name: "Razer Cynosa V2", rows: 6, cols: 22 },
    Model { pid: 0x0204, name: "Razer DeathStalker Chroma", rows: 1, cols: 12 },
    Model { pid: 0x0295, name: "Razer DeathStalker V2", rows: 6, cols: 22 },
    Model { pid: 0x0292, name: "Razer DeathStalker V2 Pro (Wired)", rows: 6, cols: 22 },
    Model { pid: 0x0290, name: "Razer DeathStalker V2 Pro (Wireless)", rows: 6, cols: 22 },
    Model { pid: 0x0298, name: "Razer DeathStalker V2 Pro TKL (Wired)", rows: 6, cols: 17 },
    Model { pid: 0x0296, name: "Razer DeathStalker V2 Pro TKL (Wireless)", rows: 6, cols: 17 },
    Model { pid: 0x0227, name: "Razer Huntsman", rows: 6, cols: 22 },
    Model { pid: 0x0226, name: "Razer Huntsman Elite", rows: 9, cols: 22 },
    Model { pid: 0x0257, name: "Razer Huntsman Mini", rows: 5, cols: 15 },
    Model { pid: 0x0269, name: "Razer Huntsman Mini (JP)", rows: 5, cols: 15 },
    Model { pid: 0x0282, name: "Razer Huntsman Mini Analog", rows: 5, cols: 15 },
    Model { pid: 0x0243, name: "Razer Huntsman Tournament Edition", rows: 6, cols: 18 },
    Model { pid: 0x026C, name: "Razer Huntsman V2", rows: 6, cols: 22 },
    Model { pid: 0x0266, name: "Razer Huntsman V2 Analog", rows: 8, cols: 22 },
    Model { pid: 0x026B, name: "Razer Huntsman V2 Tenkeyless", rows: 6, cols: 18 },
    Model { pid: 0x02A6, name: "Razer Huntsman V3 Pro", rows: 6, cols: 22 },
    Model { pid: 0x02CF, name: "Razer Huntsman V3 Pro 8KHz", rows: 6, cols: 22 },
    Model { pid: 0x02B0, name: "Razer Huntsman V3 Pro Mini", rows: 5, cols: 15 },
    Model { pid: 0x02A7, name: "Razer Huntsman V3 Pro TKL", rows: 6, cols: 22 },
    Model { pid: 0x0207, name: "Razer Orbweaver Chroma", rows: 5, cols: 22 },
    Model { pid: 0x021F, name: "Razer Ornata", rows: 6, cols: 22 },
    Model { pid: 0x021E, name: "Razer Ornata Chroma", rows: 6, cols: 22 },
    Model { pid: 0x025D, name: "Razer Ornata V2", rows: 6, cols: 22 },
    Model { pid: 0x02A1, name: "Razer Ornata V3", rows: 1, cols: 10 },
    Model { pid: 0x028F, name: "Razer Ornata V3", rows: 1, cols: 10 },
    Model { pid: 0x02A3, name: "Razer Ornata V3 Tenkeyless", rows: 1, cols: 8 },
    Model { pid: 0x0294, name: "Razer Ornata V3 X", rows: 1, cols: 1 },
    Model { pid: 0x02A2, name: "Razer Ornata V3 X", rows: 1, cols: 1 },
    Model { pid: 0x0244, name: "Razer Tartarus Pro", rows: 1, cols: 21 },
    Model { pid: 0x022B, name: "Razer Tartarus V2", rows: 4, cols: 6 },
];

/// The model with this USB product ID, if Keyglow knows it.
pub fn by_pid(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

impl Model {
    pub fn layout(&self) -> Layout {
        Layout::for_size(self.rows as usize, self.cols as usize)
    }

    pub fn tested(&self) -> bool {
        self.pid == TESTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{MAX_COLS, MAX_ROWS};

    #[test]
    fn the_tested_keyboard_is_a_six_by_eighteen_tenkeyless() {
        let m = by_pid(TESTED).unwrap();
        assert_eq!(m.name, "Razer BlackWidow V3 Tenkeyless");
        assert!(m.tested());
        assert_eq!(m.layout(), Layout::tenkeyless());
    }

    #[test]
    fn full_size_boards_use_twenty_two_columns() {
        let m = by_pid(0x024E).unwrap();
        assert_eq!((m.name, m.rows, m.cols), ("Razer BlackWidow V3", 6, 22));
        assert!(!m.tested());
    }

    #[test]
    fn every_model_fits_the_frame_and_appears_once() {
        let mut pids = std::collections::HashSet::new();
        for m in MODELS {
            assert!(pids.insert(m.pid), "{:04X} twice", m.pid);
            assert!(!m.name.is_empty() && m.name.starts_with("Razer "), "{}", m.name);
            assert!((1..=MAX_ROWS).contains(&(m.rows as usize)) && (1..=MAX_COLS).contains(&(m.cols as usize)), "{}", m.name);
            let l = m.layout();
            assert_eq!((l.rows, l.cols), (m.rows as usize, m.cols as usize), "{}", m.name);
        }
        assert!(by_pid(0x0084).is_none(), "a mouse");
        assert!(MODELS.windows(2).all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
    }
}
