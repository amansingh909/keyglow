//! One inotify instance for the settings folder (reload when `active.json` is written or
//! replaced) and the key event link folder (reopen when this keyboard's links appear).

use crate::input;
use nix::errno::Errno;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use std::fs;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};
use std::path::{Path, PathBuf};

pub const ACTIVE_FILE: &str = "active.json";

/// What the events read since the last call ask for.
#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    /// Reload `active.json`.
    pub settings: bool,
    /// Reopen the key event files.
    pub inputs: bool,
    /// The settings folder went away and could not be watched again.
    pub error: Option<String>,
}

pub struct Watcher {
    inotify: Inotify,
    config_dir: PathBuf,
    config_wd: Option<WatchDescriptor>,
    input_wd: Option<WatchDescriptor>,
}

impl Watcher {
    /// Watches the key event link folder if it exists. The settings folder is watched by
    /// [`Watcher::watch_config`].
    pub fn new(config_dir: &Path, input_dir: &Path) -> io::Result<Watcher> {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)?;
        // udev makes the by-id links with a rename (newer) or directly (older).
        let input_wd = inotify
            .add_watch(input_dir, AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO | AddWatchFlags::IN_ONLYDIR)
            .ok();
        Ok(Watcher { inotify, config_dir: config_dir.to_path_buf(), config_wd: None, input_wd })
    }

    /// Creates the settings folder if it doesn't exist yet, then watches it.
    ///
    /// The folder is created rather than waited for: it is where the settings window saves
    /// anyway, an empty folder changes nothing, and it saves watching the parents.
    pub fn watch_config(&mut self) -> io::Result<()> {
        if let Some(wd) = self.config_wd.take() {
            let _ = self.inotify.rm_watch(wd);
        }
        fs::create_dir_all(&self.config_dir)?;
        let mask = AddWatchFlags::IN_CLOSE_WRITE
            | AddWatchFlags::IN_MOVED_TO
            | AddWatchFlags::IN_DELETE_SELF
            | AddWatchFlags::IN_MOVE_SELF
            | AddWatchFlags::IN_ONLYDIR;
        self.config_wd = Some(self.inotify.add_watch(&self.config_dir, mask)?);
        Ok(())
    }

    /// Reads every pending event without blocking.
    pub fn read(&mut self) -> Changes {
        let mut changes = Changes::default();
        let mut lost_config = false;
        loop {
            let events = match self.inotify.read_events() {
                Ok(events) => events,
                Err(Errno::EINTR) => continue,
                Err(Errno::EAGAIN) => break,
                Err(e) => {
                    changes.error = Some(format!("can't read settings folder events: {e}"));
                    break;
                }
            };
            for ev in events {
                if ev.mask.contains(AddWatchFlags::IN_Q_OVERFLOW) {
                    // Events were dropped: assume anything could have changed.
                    changes.settings = true;
                    changes.inputs = true;
                } else if Some(ev.wd) == self.config_wd {
                    if ev.mask.intersects(
                        AddWatchFlags::IN_DELETE_SELF | AddWatchFlags::IN_MOVE_SELF | AddWatchFlags::IN_IGNORED,
                    ) {
                        lost_config = true;
                    } else if ev.name.as_deref() == Some(ACTIVE_FILE.as_ref()) {
                        changes.settings = true;
                    }
                } else if Some(ev.wd) == self.input_wd && ev.name.as_deref().is_some_and(input::is_keyboard_link) {
                    changes.inputs = true;
                }
            }
        }
        if lost_config {
            // Watch the path again (a moved folder would otherwise keep being watched under
            // its new name) and reload, in case active.json changed in between.
            changes.settings = true;
            if let Err(e) = self.watch_config() {
                changes.error = Some(format!("can't watch {} again: {e}", self.config_dir.display()));
            }
        }
        changes
    }
}

impl AsFd for Watcher {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inotify.as_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Dirs {
        _tmp: tempfile::TempDir,
        config: PathBuf,
        input: PathBuf,
    }

    fn dirs() -> Dirs {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config").join("keyglow");
        let input = tmp.path().join("by-id");
        fs::create_dir(&input).unwrap();
        Dirs { _tmp: tmp, config, input }
    }

    fn settings() -> Changes {
        Changes { settings: true, ..Changes::default() }
    }

    #[test]
    fn creates_a_missing_settings_folder() {
        let d = dirs();
        let mut w = Watcher::new(&d.config, &d.input).unwrap();
        w.watch_config().unwrap();
        assert!(d.config.is_dir());
        assert_eq!(w.read(), Changes::default());
    }

    #[test]
    fn reacts_to_active_json_only() {
        let d = dirs();
        let mut w = Watcher::new(&d.config, &d.input).unwrap();
        w.watch_config().unwrap();
        // An atomic save: the temporary file is ignored, the rename counts.
        let tmp = d.config.join(".active.json.tmp.42");
        fs::write(&tmp, "{}").unwrap();
        assert_eq!(w.read(), Changes::default());
        fs::rename(&tmp, d.config.join(ACTIVE_FILE)).unwrap();
        assert_eq!(w.read(), settings());
        // Written in place.
        fs::write(d.config.join(ACTIVE_FILE), "{}").unwrap();
        assert_eq!(w.read(), settings());
        // Other files in the folder.
        fs::create_dir(d.config.join("setups")).unwrap();
        fs::write(d.config.join("setups").join("a.json"), "{}").unwrap();
        fs::write(d.config.join("active.json.bak"), "{}").unwrap();
        assert_eq!(w.read(), Changes::default());
    }

    #[test]
    fn watches_the_folder_again_after_it_is_removed_or_moved() {
        let d = dirs();
        let mut w = Watcher::new(&d.config, &d.input).unwrap();
        w.watch_config().unwrap();
        fs::write(d.config.join(ACTIVE_FILE), "{}").unwrap();
        w.read();

        fs::remove_dir_all(&d.config).unwrap();
        assert_eq!(w.read(), settings());
        assert!(d.config.is_dir());
        fs::write(d.config.join(ACTIVE_FILE), "{}").unwrap();
        assert_eq!(w.read(), settings());

        let moved = d.config.with_file_name("keyglow.old");
        fs::rename(&d.config, &moved).unwrap();
        assert_eq!(w.read(), settings());
        fs::write(moved.join(ACTIVE_FILE), "{}").unwrap();
        assert_eq!(w.read(), Changes::default(), "the moved folder is no longer watched");
        fs::write(d.config.join(ACTIVE_FILE), "{}").unwrap();
        assert_eq!(w.read(), settings());
    }

    #[test]
    fn notices_new_key_event_links() {
        let d = dirs();
        let mut w = Watcher::new(&d.config, &d.input).unwrap();
        w.watch_config().unwrap();
        fs::write(d.input.join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-if02-event-mouse"), "").unwrap();
        assert_eq!(w.read(), Changes::default());
        std::os::unix::fs::symlink("../event7", d.input.join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd"))
            .unwrap();
        assert_eq!(w.read(), Changes { inputs: true, ..Changes::default() });
    }

    #[test]
    fn a_missing_input_folder_is_fine() {
        let d = dirs();
        let mut w = Watcher::new(&d.config, &d.input.join("missing")).unwrap();
        w.watch_config().unwrap();
        assert_eq!(w.read(), Changes::default());
    }
}
