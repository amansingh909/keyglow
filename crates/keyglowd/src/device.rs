//! The keyboard being lit: through the OpenRazer daemon, or through the razerkbd driver's
//! files under /sys while the daemon isn't running.

use crate::bus::Bus;
use crate::openrazer;
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

/// The USB product ID in a Razer interface folder name.
fn pid_of(name: &str) -> Option<u16> {
    if !name.get(..RAZER_PREFIX.len())?.eq_ignore_ascii_case(RAZER_PREFIX) {
        return None;
    }
    let (pid, n) = name[RAZER_PREFIX.len()..].split_once('.')?;
    if pid.len() != 4 || n.is_empty() {
        return None;
    }
    u16::from_str_radix(pid, 16).ok()
}

/// The model of a Razer interface folder name, if Keyglow knows it.
fn model_of(name: &str) -> Option<&'static Model> {
    keyboards::by_pid(pid_of(name)?)
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

/// The interface folder of the keyboard with this product ID, for a keyboard lit through the
/// daemon: key event links are named after its USB device, and the brightness is read from
/// it. The one with the brightness file when there is one, else the first by name.
pub fn folder_of(sysfs_dir: &Path, pid: u16) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(sysfs_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| pid_of(&e.file_name().to_string_lossy()) == Some(pid))
        .map(|e| e.path())
        .collect();
    found.sort();
    let lit = found.iter().position(|p| p.join(BRIGHTNESS_FILE).exists()).unwrap_or(0);
    found.into_iter().nth(lit)
}

/// Where a keyboard's frames go.
enum Output {
    /// The OpenRazer daemon, to the keyboard's object path.
    Daemon { bus: Bus, path: String },
    /// The driver's files, kept open so a frame costs two `pwrite`s.
    Files { frame: File, effect: File },
}

pub struct Keyboard {
    model: &'static Model,
    /// The lighting interface folder. A keyboard lit through the daemon may have none found.
    dir: Option<PathBuf>,
    output: Output,
    /// Optional: without it the keyboard is never treated as dimmed.
    brightness: Option<File>,
}

impl Keyboard {
    /// The keyboard at `path` on the OpenRazer daemon, with its interface folder if one was found.
    pub fn through_daemon(bus: Bus, path: String, model: &'static Model, dir: Option<PathBuf>) -> Keyboard {
        let brightness = dir.as_ref().and_then(|d| File::open(d.join(BRIGHTNESS_FILE)).ok());
        Keyboard { model, dir, output: Output::Daemon { bus, path }, brightness }
    }

    /// The keyboard whose lighting files are in `dir`, for when the daemon isn't running.
    pub fn open(dir: &Path, model: &'static Model) -> io::Result<Keyboard> {
        // Write only (the files are mode 0220) and never created.
        let write = |name| OpenOptions::new().write(true).open(dir.join(name));
        Ok(Keyboard {
            model,
            dir: Some(dir.to_path_buf()),
            output: Output::Files { frame: write(FRAME_FILE)?, effect: write(EFFECT_FILE)? },
            brightness: File::open(dir.join(BRIGHTNESS_FILE)).ok(),
        })
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Points a keyboard lit through the daemon at its interface folder again, after the
    /// daemon handled a replug on its own and the folder got a new name.
    pub fn set_dir(&mut self, dir: Option<PathBuf>) {
        self.brightness = dir.as_ref().and_then(|d| File::open(d.join(BRIGHTNESS_FILE)).ok());
        self.dir = dir;
    }

    pub fn model(&self) -> &'static Model {
        self.model
    }

    pub fn uses_daemon(&self) -> bool {
        matches!(self.output, Output::Daemon { .. })
    }

    /// Where the frames go, for messages: `through the OpenRazer daemon` or `at <folder>`.
    pub fn place(&self) -> String {
        match (&self.output, &self.dir) {
            (Output::Daemon { .. }, _) | (_, None) => "through the OpenRazer daemon".to_string(),
            (Output::Files { .. }, Some(dir)) => format!("at {}", dir.display()),
        }
    }

    /// Sends the frame bytes, then has the keyboard show them.
    pub fn show(&mut self, bytes: &[u8]) -> io::Result<()> {
        match &mut self.output {
            Output::Daemon { bus, path } => openrazer::show(bus, path, bytes),
            Output::Files { frame, effect } => {
                frame.write_all_at(bytes, 0)?;
                effect.write_all_at(b"1", 0)
            }
        }
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
    fn the_daemons_keyboard_folder_is_found_by_product_id() {
        let d = tempfile::tempdir().unwrap();
        fake_device(d.path(), "0003:1532:0A24.0014", &[]);
        let lit = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        fake_device(d.path(), "0003:1532:0a24.0017", &[]);
        fake_device(d.path(), "0003:1532:024E.0003", LIGHTING);
        assert_eq!(folder_of(d.path(), 0x0A24), Some(lit));
        assert_eq!(folder_of(d.path(), 0x0203), None);
        // Without a brightness file (a later driver may name it otherwise), the first folder
        // still names the USB device.
        let e = tempfile::tempdir().unwrap();
        let first = fake_device(e.path(), "0003:1532:0A24.0014", &[]);
        fake_device(e.path(), "0003:1532:0A24.0015", &[]);
        assert_eq!(folder_of(e.path(), 0x0A24), Some(first));
        assert_eq!(folder_of(&e.path().join("missing"), 0x0A24), None);
    }

    #[test]
    fn a_keyboard_on_the_files_says_where_they_are() {
        let d = tempfile::tempdir().unwrap();
        let dir = fake_device(d.path(), "0003:1532:0A24.0016", LIGHTING);
        let kb = Keyboard::open(&dir, tkl()).unwrap();
        assert!(!kb.uses_daemon());
        assert_eq!(kb.place(), format!("at {}", dir.display()));
        assert_eq!(kb.dir(), Some(dir.as_path()));
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
