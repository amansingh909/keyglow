//! The keyboard's lighting files under the razerkbd driver's sysfs folder.

use keyglow_core::keyboards::{self, Model};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// Interface folders of Razer devices are named `0003:1532:<product id>.<n>`, where `<n>`
/// changes on every replug.
const RAZER_PREFIX: &str = "0003:1532:";
pub const DEFAULT_SYSFS_DIR: &str = "/sys/bus/hid/drivers/razerkbd";
const FRAME_FILE: &str = "matrix_custom_frame";
const EFFECT_FILE: &str = "matrix_effect_custom";
const BRIGHTNESS_FILE: &str = "matrix_brightness";

/// The model of a Razer interface folder name, if Keyglow knows it.
fn model_of(name: &str) -> Option<&'static Model> {
    if !name.get(..RAZER_PREFIX.len())?.eq_ignore_ascii_case(RAZER_PREFIX) {
        return None;
    }
    let (pid, n) = name[RAZER_PREFIX.len()..].split_once('.')?;
    if pid.len() != 4 || n.is_empty() {
        return None;
    }
    keyboards::by_pid(u16::from_str_radix(pid, 16).ok()?)
}

/// The interface folder that has the lighting files of a keyboard Keyglow knows, if one is
/// plugged in, and its model. Only one of a keyboard's interfaces has them. With several
/// keyboards, the first folder by name wins.
pub fn find(sysfs_dir: &Path) -> Option<(PathBuf, &'static Model)> {
    let mut found: Vec<(PathBuf, &'static Model)> = fs::read_dir(sysfs_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.path(), model_of(&e.file_name().to_string_lossy())?)))
        .filter(|(p, _)| p.join(FRAME_FILE).exists())
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().next()
}

/// The open lighting files of one interface. They stay open so a frame costs two `pwrite`s.
pub struct Keyboard {
    dir: PathBuf,
    model: &'static Model,
    frame: File,
    effect: File,
    /// Optional: without it the keyboard is never treated as dimmed.
    brightness: Option<File>,
}

impl Keyboard {
    pub fn open(dir: &Path, model: &'static Model) -> io::Result<Keyboard> {
        // Write only (the files are mode 0220) and never created.
        let write = |name| OpenOptions::new().write(true).open(dir.join(name));
        Ok(Keyboard {
            dir: dir.to_path_buf(),
            model,
            frame: write(FRAME_FILE)?,
            effect: write(EFFECT_FILE)?,
            brightness: File::open(dir.join(BRIGHTNESS_FILE)).ok(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn model(&self) -> &'static Model {
        self.model
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

    fn tkl() -> &'static Model {
        keyboards::by_pid(keyboards::TESTED).unwrap()
    }

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
        let (dir, model) = find(d.path()).unwrap();
        assert_eq!((dir, model.name), (lit, "Razer BlackWidow V3 Tenkeyless"));
    }

    #[test]
    fn finds_other_keyboard_models() {
        let d = tempfile::tempdir().unwrap();
        let lit = fake_device(d.path(), "0003:1532:024e.0003", LIGHTING);
        let (dir, model) = find(d.path()).unwrap();
        assert_eq!((dir, model.name, model.cols), (lit, "Razer BlackWidow V3", 22));
    }

    #[test]
    fn ignores_other_razer_devices() {
        let d = tempfile::tempdir().unwrap();
        fake_device(d.path(), "0003:1532:0084.0001", LIGHTING);
        for name in ["0003:1532:0A24", "0003:1532:0A24.", "0003:1532:XYZW.0001", "0003:1532:00A24.0001", "0003:046D:0A24.0001"] {
            fake_device(d.path(), name, LIGHTING);
        }
        assert_eq!(find(d.path()), None);
        assert_eq!(find(&d.path().join("missing")), None);
    }

    #[test]
    fn show_writes_the_frame_then_custom() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        let mut kb = Keyboard::open(&dir, tkl()).unwrap();
        let mut frame = BLACK_FRAME;
        frame[2][5] = Rgb { r: 1, g: 2, b: 3 };
        let bytes = encode(&frame, 6, 18);
        kb.show(&bytes).unwrap();
        assert_eq!(fs::read(dir.join("matrix_custom_frame")).unwrap(), bytes);
        assert_eq!(fs::read(dir.join("matrix_effect_custom")).unwrap(), b"1");
        // Later frames overwrite from the start instead of appending.
        frame[2][5] = Rgb { r: 9, g: 8, b: 7 };
        let bytes = encode(&frame, 6, 18);
        kb.show(&bytes).unwrap();
        assert_eq!(fs::read(dir.join("matrix_custom_frame")).unwrap(), bytes);
        assert_eq!(fs::read(dir.join("matrix_effect_custom")).unwrap(), b"1");
    }

    #[test]
    fn open_fails_without_the_lighting_files() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0014", &[]);
        assert!(Keyboard::open(&dir, tkl()).is_err());
        // Opening must not create the files.
        assert!(!dir.join("matrix_custom_frame").exists());
    }

    #[test]
    fn brightness_reads_the_number() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        let mut kb = Keyboard::open(&dir, tkl()).unwrap();
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
