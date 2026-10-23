//! Key presses from the keyboard's event files. Only grid positions are wanted from them:
//! key codes are never logged, printed or stored.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const DEFAULT_INPUT_DIR: &str = "/dev/input/by-id";
/// udev names every Razer keyboard interface's key event link `usb-Razer...event-kbd`.
const RAZER_PREFIX: &[u8] = b"usb-Razer";
const LINK_SUFFIX: &[u8] = b"event-kbd";

/// Size of `struct input_event` on x86_64: 16 bytes of time, u16 type, u16 code, i32 value.
pub const EVENT_SIZE: usize = 24;
const EV_KEY: u16 = 1;
const KEY_DOWN: i32 = 1;

/// Whether a `/dev/input/by-id` name is the key event link of a Razer keyboard.
pub fn is_keyboard_link(name: &OsStr) -> bool {
    let name = name.as_bytes();
    name.len() >= RAZER_PREFIX.len() + LINK_SUFFIX.len() && name.starts_with(RAZER_PREFIX) && name.ends_with(LINK_SUFFIX)
}

/// How udev's by-id link names for a keyboard start: `usb-`, its USB manufacturer, product
/// and (when it has one) serial strings joined by `_`, then `-`. They are read from the USB
/// device two folders above the lighting interface. None when they can't be read.
pub fn link_prefix(lighting_dir: &Path) -> Option<String> {
    let usb = fs::canonicalize(lighting_dir).ok()?.parent()?.parent()?.to_path_buf();
    let read = |file: &str| fs::read_to_string(usb.join(file)).ok().map(|s| udev_clean(&s)).filter(|s| !s.is_empty());
    let mut id = format!("{}_{}", read("manufacturer")?, read("product")?);
    if let Some(serial) = read("serial") {
        id = format!("{id}_{serial}");
    }
    Some(format!("usb-{id}-"))
}

/// A USB string as udev puts it into a by-id name: each run of whitespace becomes one `_`
/// (none at the ends), and anything but ASCII letters, digits and `#+-.:=@_` becomes `_`.
fn udev_clean(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join("_");
    joined.chars().map(|c| if c.is_ascii_alphanumeric() || "#+-.:=@_".contains(c) { c } else { '_' }).collect()
}

/// The keyboard's key event files, sorted: the Razer keyboard links that start with `prefix`
/// (see [`link_prefix`]). Without a prefix, or when none start with it, every Razer keyboard's.
pub fn find(input_dir: &Path, prefix: Option<&str>) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(input_dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> =
        entries.filter_map(|e| e.ok()).filter(|e| is_keyboard_link(&e.file_name())).map(|e| e.path()).collect();
    found.sort();
    let own = |p: &PathBuf| prefix.is_some_and(|pre| p.file_name().is_some_and(|n| n.as_bytes().starts_with(pre.as_bytes())));
    if found.iter().any(own) {
        found.retain(own);
    }
    found
}

/// Key codes of the key-down events among whole `input_event` records (repeats and
/// releases are left out). A trailing partial record is ignored.
pub fn parse(buf: &[u8]) -> Vec<u16> {
    let (records, _partial) = buf.as_chunks::<EVENT_SIZE>();
    records
        .iter()
        .filter_map(|r| {
            let kind = u16::from_ne_bytes([r[16], r[17]]);
            let code = u16::from_ne_bytes([r[18], r[19]]);
            let value = i32::from_ne_bytes([r[20], r[21], r[22], r[23]]);
            (kind == EV_KEY && value == KEY_DOWN).then_some(code)
        })
        .collect()
}

/// Joins reads that end partway through a record.
#[derive(Default)]
pub struct Records {
    partial: Vec<u8>,
}

impl Records {
    /// Takes the next bytes read and returns the key-down codes of the records they complete.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<u16> {
        if self.partial.is_empty() {
            let whole = bytes.len() - bytes.len() % EVENT_SIZE;
            self.partial.extend_from_slice(&bytes[whole..]);
            return parse(&bytes[..whole]);
        }
        self.partial.extend_from_slice(bytes);
        let whole = self.partial.len() - self.partial.len() % EVENT_SIZE;
        let codes = parse(&self.partial[..whole]);
        self.partial.drain(..whole);
        codes
    }
}

/// One open key event file, read without grabbing it so typing still works everywhere.
pub struct KeyFile {
    path: PathBuf,
    file: File,
    records: Records,
}

pub enum Read {
    /// Everything available was read; the file stays open.
    Open(Vec<u16>),
    /// The device went away or the file ended; close it. Holds the presses read before that.
    Gone(Vec<u16>),
}

impl KeyFile {
    pub fn open(path: &Path) -> io::Result<KeyFile> {
        let file = OpenOptions::new().read(true).custom_flags(nix::libc::O_NONBLOCK).open(path)?;
        Ok(KeyFile { path: path.to_path_buf(), file, records: Records::default() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self) -> &File {
        &self.file
    }

    /// Reads until the file would block.
    pub fn read(&mut self) -> Read {
        let mut codes = Vec::new();
        let mut buf = [0u8; EVENT_SIZE * 32];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => return Read::Gone(codes),
                Ok(n) => codes.extend(self.records.push(&buf[..n])),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Read::Open(codes),
                Err(_) => return Read::Gone(codes),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn record(kind: u16, code: u16, value: i32) -> Vec<u8> {
        let mut r = vec![0u8; 16];
        r.extend_from_slice(&kind.to_ne_bytes());
        r.extend_from_slice(&code.to_ne_bytes());
        r.extend_from_slice(&value.to_ne_bytes());
        assert_eq!(r.len(), EVENT_SIZE);
        r
    }

    #[test]
    fn parse_reads_key_presses_only() {
        let mut buf = Vec::new();
        buf.extend(record(4, 4, 30)); // EV_MSC scan code
        buf.extend(record(EV_KEY, 30, 1)); // press
        buf.extend(record(0, 0, 0)); // EV_SYN
        buf.extend(record(EV_KEY, 30, 2)); // repeat
        buf.extend(record(EV_KEY, 30, 0)); // release
        buf.extend(record(0, 0, 0));
        assert_eq!(parse(&buf), vec![30]);
        buf.extend(record(EV_KEY, 57, 1));
        buf.extend(&record(EV_KEY, 1, 1)[..10]);
        assert_eq!(parse(&buf), vec![30, 57]);
        assert_eq!(parse(&[]), Vec::<u16>::new());
    }

    #[test]
    fn records_join_reads_split_mid_record() {
        let mut bytes = record(EV_KEY, 30, 1);
        bytes.extend(record(EV_KEY, 31, 1));
        bytes.extend(record(EV_KEY, 32, 1));
        let mut r = Records::default();
        assert_eq!(r.push(&bytes[..10]), Vec::<u16>::new());
        assert_eq!(r.push(&bytes[10..30]), vec![30]);
        assert_eq!(r.push(&bytes[30..47]), Vec::<u16>::new());
        assert_eq!(r.push(&bytes[47..]), vec![31, 32]);
        assert_eq!(r.push(&[]), Vec::<u16>::new());
    }

    #[test]
    fn finds_both_keyboard_links_only() {
        let d = tempfile::tempdir().unwrap();
        for name in [
            "usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-if03",
            "usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd",
            "usb-Razer_Razer_BlackWidow_V3_Tenkeyless-if01-event-kbd",
            "usb-Razer_Razer_BlackWidow_V3_Tenkeyless-if02-event-mouse",
            "usb-Other_Keyboard-event-kbd",
            "usb-Razer_Razer_DeathAdder_V2-if01-event-kbd",
        ] {
            fs::write(d.path().join(name), "").unwrap();
        }
        let tkl = vec![
            d.path().join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd"),
            d.path().join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-if01-event-kbd"),
        ];
        assert_eq!(find(d.path(), Some("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-")), tkl);
        // Without the keyboard's own names, or before its links exist, any Razer keyboard's.
        let mut any = tkl.clone();
        any.push(d.path().join("usb-Razer_Razer_DeathAdder_V2-if01-event-kbd"));
        assert_eq!(find(d.path(), None), any);
        assert_eq!(find(d.path(), Some("usb-Razer_Razer_Huntsman_V2-")), any);
        // A longer model name isn't mistaken for a shorter one.
        assert_eq!(find(d.path(), Some("usb-Razer_Razer_BlackWidow_V3-")), any);
        assert!(find(&d.path().join("missing"), None).is_empty());
    }

    #[test]
    fn link_prefix_follows_udev_naming() {
        let d = tempfile::tempdir().unwrap();
        let usb = d.path().join("usb1").join("1-3");
        let lighting = usb.join("1-3:1.2").join("0003:1532:024E.0005");
        fs::create_dir_all(&lighting).unwrap();
        fs::write(usb.join("manufacturer"), "Razer\n").unwrap();
        fs::write(usb.join("product"), "Razer BlackWidow V3\n").unwrap();
        // The driver folder holds a link to the interface, like /sys/bus/hid/drivers/razerkbd.
        let drivers = d.path().join("razerkbd");
        fs::create_dir(&drivers).unwrap();
        std::os::unix::fs::symlink(&lighting, drivers.join("0003:1532:024E.0005")).unwrap();
        assert_eq!(link_prefix(&drivers.join("0003:1532:024E.0005")).as_deref(), Some("usb-Razer_Razer_BlackWidow_V3-"));
        fs::write(usb.join("serial"), "  PM2 21/4  \n").unwrap();
        assert_eq!(link_prefix(&lighting).as_deref(), Some("usb-Razer_Razer_BlackWidow_V3_PM2_21_4-"));
        fs::remove_file(usb.join("product")).unwrap();
        assert_eq!(link_prefix(&lighting), None);
        assert_eq!(link_prefix(&d.path().join("missing")), None);
    }

    #[test]
    fn a_file_that_ends_is_gone() {
        // A regular file stands in for an event file here: it ends instead of blocking.
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd");
        let mut bytes = record(EV_KEY, 30, 1);
        bytes.extend(record(EV_KEY, 30, 0));
        bytes.extend(record(EV_KEY, 44, 1));
        fs::write(&path, &bytes).unwrap();
        let mut f = KeyFile::open(&path).unwrap();
        assert!(matches!(f.read(), Read::Gone(codes) if codes == vec![30, 44]));
    }
}
