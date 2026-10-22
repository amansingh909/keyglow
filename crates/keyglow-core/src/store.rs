use crate::setup::Setup;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum StoreError {
    BadName(String),
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::BadName(n) => write!(f, "bad setup name {n:?}"),
            StoreError::Io(e) => write!(f, "{e}"),
            StoreError::Json(e) => write!(f, "bad settings file: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub pid: u32,
    pub keyboard: bool,
    pub detail: String,
}

fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.chars().count() > 64 || name.contains('/') || name.contains('\0') || name.starts_with('.') {
        return Err(StoreError::BadName(name.to_string()));
    }
    Ok(())
}

/// Writes next to the target (same directory) and renames over it.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{file}.tmp.{}", std::process::id()));
    let res = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn default_dir() -> PathBuf {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(p) if !p.is_empty() => PathBuf::from(p),
            _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"),
        };
        base.join("keyglow")
    }

    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn active_path(&self) -> PathBuf {
        self.dir.join("active.json")
    }

    fn setups_dir(&self) -> PathBuf {
        self.dir.join("setups")
    }

    fn setup_path(&self, name: &str) -> Result<PathBuf> {
        check_name(name)?;
        Ok(self.setups_dir().join(format!("{name}.json")))
    }

    /// A missing file gives the owner default; unreadable or invalid JSON is an error.
    pub fn load_active(&self) -> Result<Setup> {
        match fs::read(self.active_path()) {
            Ok(b) => Ok(serde_json::from_slice::<Setup>(&b)?.sanitized()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Setup::owner_default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_active(&self, setup: &Setup) -> Result<()> {
        write_atomic(&self.active_path(), &serde_json::to_vec_pretty(setup)?)?;
        Ok(())
    }

    pub fn list_setups(&self) -> Result<Vec<String>> {
        let rd = match fs::read_dir(self.setups_dir()) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in rd {
            let file = entry?.file_name().to_string_lossy().into_owned();
            if let Some(stem) = file.strip_suffix(".json") {
                if check_name(stem).is_ok() {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn load_setup(&self, name: &str) -> Result<Setup> {
        Ok(serde_json::from_slice::<Setup>(&fs::read(self.setup_path(name)?)?)?.sanitized())
    }

    pub fn save_setup(&self, setup: &Setup) -> Result<()> {
        let path = self.setup_path(&setup.name)?;
        write_atomic(&path, &serde_json::to_vec_pretty(setup)?)?;
        Ok(())
    }

    /// Refuses to overwrite an existing setup.
    pub fn rename_setup(&self, old: &str, new: &str) -> Result<()> {
        let (old_path, new_path) = (self.setup_path(old)?, self.setup_path(new)?);
        if old == new {
            return Ok(());
        }
        if new_path.exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("setup {new:?} already exists")).into());
        }
        let mut setup = self.load_setup(old)?;
        setup.name = new.to_string();
        self.save_setup(&setup)?;
        fs::remove_file(old_path)?;
        Ok(())
    }

    pub fn delete_setup(&self, name: &str) -> Result<()> {
        fs::remove_file(self.setup_path(name)?)?;
        Ok(())
    }
}

/// `$XDG_RUNTIME_DIR/keyglow/status.json`, or `/run/user/<uid>/keyglow/status.json`.
pub fn status_path() -> PathBuf {
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            use std::os::unix::fs::MetadataExt;
            let uid = fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0);
            PathBuf::from(format!("/run/user/{uid}"))
        }
    };
    base.join("keyglow").join("status.json")
}

pub fn write_status(path: &Path, status: &Status) -> Result<()> {
    write_atomic(path, &serde_json::to_vec(status)?)?;
    Ok(())
}

pub fn read_status(path: &Path) -> Option<Status> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::Setup;
    fn store() -> (tempfile::TempDir, Store) {
        let d = tempfile::tempdir().unwrap();
        let s = Store::new(d.path().join("keyglow"));
        (d, s)
    }
    #[test]
    fn missing_active_file_gives_the_owner_default() {
        let (_d, s) = store();
        assert_eq!(s.load_active().unwrap(), Setup::owner_default());
    }
    #[test]
    fn save_active_round_trips_and_leaves_no_temp_file_behind() {
        let (_d, s) = store();
        let mut x = Setup::owner_default();
        x.fps = 45;
        s.save_active(&x).unwrap();
        assert_eq!(s.load_active().unwrap(), x);
        let dir = s.active_path().parent().unwrap().to_path_buf();
        let names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("active.json")]);
    }
    #[test]
    fn setups_save_list_rename_delete() {
        let (_d, s) = store();
        assert_eq!(s.list_setups().unwrap(), Vec::<String>::new());
        let mut a = Setup::owner_default();
        a.name = "zeta".into();
        let mut b = Setup::owner_default();
        b.name = "alpha".into();
        s.save_setup(&a).unwrap();
        s.save_setup(&b).unwrap();
        assert_eq!(s.list_setups().unwrap(), vec!["alpha", "zeta"]);
        s.rename_setup("zeta", "beta").unwrap();
        assert_eq!(s.list_setups().unwrap(), vec!["alpha", "beta"]);
        assert_eq!(s.load_setup("beta").unwrap().name, "beta");
        s.delete_setup("alpha").unwrap();
        assert_eq!(s.list_setups().unwrap(), vec!["beta"]);
    }
    #[test]
    fn bad_setup_names_are_refused() {
        let (_d, s) = store();
        for n in ["", "../x", ".hidden", "a/b", "a\0b"] {
            assert!(matches!(s.load_setup(n), Err(StoreError::BadName(_))), "{n:?}");
            assert!(matches!(s.delete_setup(n), Err(StoreError::BadName(_))), "{n:?}");
        }
        assert!(matches!(s.load_setup(&"x".repeat(65)), Err(StoreError::BadName(_))));
        let mut x = Setup::owner_default();
        x.name = "a/b".into();
        assert!(matches!(s.save_setup(&x), Err(StoreError::BadName(_))));
        assert!(matches!(s.rename_setup("ok", "../x"), Err(StoreError::BadName(_))));
    }
    #[test]
    fn bad_json_is_an_error_not_a_default() {
        let (_d, s) = store();
        std::fs::create_dir_all(s.active_path().parent().unwrap()).unwrap();
        std::fs::write(s.active_path(), "{ nope").unwrap();
        assert!(matches!(s.load_active(), Err(StoreError::Json(_))));
    }
    #[test]
    fn status_round_trips_and_missing_is_none() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("run").join("keyglow").join("status.json");
        assert!(read_status(&p).is_none());
        let st = Status { pid: 42, keyboard: true, detail: "ok".into() };
        write_status(&p, &st).unwrap();
        assert_eq!(read_status(&p), Some(st));
    }
    #[test]
    fn hand_edited_active_file_with_gaps_loads() {
        let (_d, s) = store();
        std::fs::create_dir_all(s.active_path().parent().unwrap()).unwrap();
        std::fs::write(s.active_path(), r#"{"fps":5000,"layers":[{"opacity":300,"effect":{"type":"spectrum"}}]}"#).unwrap();
        let x = s.load_active().unwrap();
        assert_eq!(x.name, "Default");
        assert_eq!(x.brightness, 100);
        assert_eq!(x.fps, 60);
        assert_eq!(x.layers[0].opacity, 100);
        std::fs::write(s.active_path(), r#"{"layers":[{"effect":{"type":"disco"}}]}"#).unwrap();
        assert!(matches!(s.load_active(), Err(StoreError::Json(_))));
    }
}
