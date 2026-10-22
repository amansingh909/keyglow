//! The keyboard's lighting files under the razerkbd driver's sysfs folder.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// Interface folders of the BlackWidow V3 Tenkeyless are named `0003:1532:0A24.<n>`,
/// where `<n>` changes on every replug.
pub const DEVICE_PREFIX: &str = "0003:1532:0A24.";
pub const DEFAULT_SYSFS_DIR: &str = "/sys/bus/hid/drivers/razerkbd";
const FRAME_FILE: &str = "matrix_custom_frame";
const EFFECT_FILE: &str = "matrix_effect_custom";
const BRIGHTNESS_FILE: &str = "matrix_brightness";

/// The interface folder that has the lighting files, if the keyboard is plugged in.
/// Only one of the keyboard's four interfaces has them.
pub fn find(sysfs_dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(sysfs_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.len() > DEVICE_PREFIX.len()
                && name.is_char_boundary(DEVICE_PREFIX.len())
                && name[..DEVICE_PREFIX.len()].eq_ignore_ascii_case(DEVICE_PREFIX)
        })
        .map(|e| e.path())
        .filter(|p| p.join(FRAME_FILE).exists())
        .collect();
    found.sort();
    found.into_iter().next()
}

/// The open lighting files of one interface. They stay open so a frame costs two `pwrite`s.
pub struct Keyboard {
    dir: PathBuf,
    frame: File,
    effect: File,
    /// Optional: without it the keyboard is never treated as dimmed.
    brightness: Option<File>,
}

impl Keyboard {
    pub fn open(dir: &Path) -> io::Result<Keyboard> {
        // Write only (the files are mode 0220) and never created.
        let write = |name| OpenOptions::new().write(true).open(dir.join(name));
        Ok(Keyboard {
            dir: dir.to_path_buf(),
            frame: write(FRAME_FILE)?,
            effect: write(EFFECT_FILE)?,
            brightness: File::open(dir.join(BRIGHTNESS_FILE)).ok(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes the frame bytes, then `1` to show them.
    pub fn show(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.frame.write_all_at(bytes, 0)?;
        self.effect.write_all_at(b"1", 0)
    }

    /// The hardware brightness, 0 to 255. OpenRazer sets it to 0 while the screen saver runs.
    /// Only ever read: Keyglow never writes it.
    pub fn brightness(&mut self) -> io::Result<u8> {
        let file = self
            .brightness
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no matrix_brightness"))?;
        let mut buf = [0u8; 16];
        let n = loop {
            match file.read_at(&mut buf, 0) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                other => break other?,
            }
        };
        std::str::from_utf8(&buf[..n])
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "matrix_brightness is not a number from 0 to 255"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyglow_core::color::Rgb;
    use keyglow_core::frame::{encode, BLACK_FRAME};
    use std::fs;

    fn fake_device(root: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, contents) in files {
            fs::write(dir.join(file), contents).unwrap();
        }
        dir
    }

    const LIGHTING: &[(&str, &str)] =
        &[("matrix_custom_frame", ""), ("matrix_effect_custom", ""), ("matrix_brightness", "255\n")];

    #[test]
    fn finds_the_lighting_interface_among_several() {
        let d = tempfile::tempdir().unwrap();
        fake_device(d.path(), "0003:1532:0A24.0014", &[]);
        fake_device(d.path(), "0003:1532:0A24.0015", &[("device_type", "x\n")]);
        let lit = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        fake_device(d.path(), "0003:1532:0A24.0017", &[]);
        fs::write(d.path().join("bind"), "").unwrap();
        assert_eq!(find(d.path()), Some(lit));
    }

    #[test]
    fn ignores_other_razer_devices() {
        let d = tempfile::tempdir().unwrap();
        fake_device(d.path(), "0003:1532:0084.0001", LIGHTING);
        assert_eq!(find(d.path()), None);
        assert_eq!(find(&d.path().join("missing")), None);
    }

    #[test]
    fn show_writes_the_frame_then_custom() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        let mut kb = Keyboard::open(&dir).unwrap();
        let mut frame = BLACK_FRAME;
        frame[2][5] = Rgb { r: 1, g: 2, b: 3 };
        let bytes = encode(&frame);
        kb.show(&bytes).unwrap();
        assert_eq!(fs::read(dir.join("matrix_custom_frame")).unwrap(), bytes);
        assert_eq!(fs::read(dir.join("matrix_effect_custom")).unwrap(), b"1");
        // Later frames overwrite from the start instead of appending.
        frame[2][5] = Rgb { r: 9, g: 8, b: 7 };
        let bytes = encode(&frame);
        kb.show(&bytes).unwrap();
        assert_eq!(fs::read(dir.join("matrix_custom_frame")).unwrap(), bytes);
        assert_eq!(fs::read(dir.join("matrix_effect_custom")).unwrap(), b"1");
    }

    #[test]
    fn open_fails_without_the_lighting_files() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0014", &[]);
        assert!(Keyboard::open(&dir).is_err());
        // Opening must not create the files.
        assert!(!dir.join("matrix_custom_frame").exists());
    }

    #[test]
    fn brightness_reads_the_number() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        let mut kb = Keyboard::open(&dir).unwrap();
        assert_eq!(kb.brightness().unwrap(), 255);
        // The file stays open, and each read starts from the beginning.
        fs::write(dir.join("matrix_brightness"), "0\n").unwrap();
        assert_eq!(kb.brightness().unwrap(), 0);
        fs::write(dir.join("matrix_brightness"), "128").unwrap();
        assert_eq!(kb.brightness().unwrap(), 128);
        fs::write(dir.join("matrix_brightness"), "lots\n").unwrap();
        assert!(kb.brightness().is_err());
        fs::write(dir.join("matrix_brightness"), "256\n").unwrap();
        assert!(kb.brightness().is_err());
    }
}
