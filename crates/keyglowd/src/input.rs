//! Key presses from the keyboard's event files. Only grid positions are wanted from them:
//! key codes are never logged, printed or stored.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const DEFAULT_INPUT_DIR: &str = "/dev/input/by-id";
const LINK_PREFIX: &[u8] = b"usb-Razer_Razer_BlackWidow_V3_Tenkeyless-";
const LINK_SUFFIX: &[u8] = b"event-kbd";

/// Size of `struct input_event` on x86_64: 16 bytes of time, u16 type, u16 code, i32 value.
pub const EVENT_SIZE: usize = 24;
const EV_KEY: u16 = 1;
const KEY_DOWN: i32 = 1;

/// Whether a `/dev/input/by-id` name is one of this keyboard's key event links.
pub fn is_keyboard_link(name: &OsStr) -> bool {
    let name = name.as_bytes();
    name.len() >= LINK_PREFIX.len() + LINK_SUFFIX.len() && name.starts_with(LINK_PREFIX) && name.ends_with(LINK_SUFFIX)
}

/// The keyboard's key event files, sorted.
pub fn find(input_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(input_dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> =
        entries.filter_map(|e| e.ok()).filter(|e| is_keyboard_link(&e.file_name())).map(|e| e.path()).collect();
    found.sort();
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
        ] {
            fs::write(d.path().join(name), "").unwrap();
        }
        assert_eq!(
            find(d.path()),
            vec![
                d.path().join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd"),
                d.path().join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-if01-event-kbd"),
            ]
        );
        assert!(find(&d.path().join("missing")).is_empty());
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
