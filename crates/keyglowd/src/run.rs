//! The main loop: sleeps in `poll` until a key press, a settings change, a signal or the
//! next frame time, and draws only while something on the keyboard moves.
//!
//! Wake-up rules (see [`next_wait`]): with no keyboard, rescan every 2 s; while the hardware
//! brightness reads 0, read it again every 2 s and draw nothing; after a key press that lights
//! something, draw at once instead of waiting for the next frame time, which can be most of a
//! frame away; while the engine animates (or a change is waiting to be shown), wake at the next
//! frame time; while still, wake only to resend the last frame every 5 s. While the keyboard is
//! there but its key event files aren't all open (or their folder isn't watched), look for them
//! every 2 s as well. Events (key presses, settings writes, new key links, signals) wake it
//! sooner. While drawing, the brightness is read at most every 2 s, just before a frame.

use crate::device::{self, Keyboard};
use crate::input::{self, KeyFile, Read};
use crate::slice;
use crate::watch::Watcher;
use keyglow_core::engine::Engine;
use keyglow_core::frame::encode;
use keyglow_core::keymap::grid_of_keycode;
use keyglow_core::setup::Setup;
use keyglow_core::store::{write_status, Status, Store};
use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use nix::sys::signal::{SigSet, Signal};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use std::io;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// How often to look for the keyboard while it is missing.
pub const RESCAN: Duration = Duration::from_secs(2);
/// How often to read the hardware brightness while drawing or dimmed.
pub const BRIGHTNESS_CHECK: Duration = Duration::from_secs(2);
/// How often to send the last frame again while nothing moves.
pub const RESEND: Duration = Duration::from_secs(5);

pub struct Options {
    pub config_dir: PathBuf,
    pub sysfs_dir: PathBuf,
    pub input_dir: PathBuf,
    pub status_file: PathBuf,
}

/// How long to sleep. Zero only when a frame is due.
pub fn next_wait(
    animating: bool,
    pressed: bool,
    keyboard: bool,
    dimmed: bool,
    since_last_frame: Duration,
    frame_interval: Duration,
) -> Duration {
    if !keyboard {
        RESCAN
    } else if dimmed {
        BRIGHTNESS_CHECK
    } else if pressed {
        Duration::ZERO
    } else if animating {
        frame_interval.saturating_sub(since_last_frame)
    } else {
        RESEND.saturating_sub(since_last_frame)
    }
}

/// A `poll` timeout in whole milliseconds, rounded up so a wait never ends early.
pub fn timeout_ms(wait: Duration) -> i32 {
    let ms = wait.as_nanos().div_ceil(1_000_000);
    i32::try_from(ms).unwrap_or(i32::MAX)
}

/// Runs until SIGTERM or SIGINT.
pub fn run(opts: &Options) -> io::Result<()> {
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGINT);
    mask.thread_block()?;
    let signals = SignalFd::with_flags(&mask, SfdFlags::SFD_NONBLOCK | SfdFlags::SFD_CLOEXEC)?;
    match slice::request() {
        Ok(true) => {}
        Ok(false) => eprintln!(
            "keyglowd: the kernel gave no short time slice (Linux 6.12 and later do), so frames may lag while the computer is busy"
        ),
        Err(e) => eprintln!("keyglowd: can't ask for a short time slice ({e}), so frames may lag while the computer is busy"),
    }
    Daemon::start(opts)?.run(&signals)
}

struct Daemon<'a> {
    opts: &'a Options,
    store: Store,
    watcher: Watcher,
    engine: Engine,
    /// The setup the engine is drawing, to skip resetting it when an identical file is saved.
    setup: Setup,
    /// The zero of the engine's clock.
    start: Instant,
    keyboard: Option<Keyboard>,
    /// How the keyboard's key event links are named, when its USB strings could be read.
    key_prefix: Option<String>,
    keys: Vec<KeyFile>,
    /// Every key event link is open and their folder is watched, so no 2 s retry is needed.
    keys_complete: bool,
    next_key_scan: Instant,
    /// Why the settings folder can't be watched, for the status file.
    watch_error: Option<String>,
    /// Why `active.json` couldn't be loaded, for the status file.
    settings_error: Option<String>,
    /// Why a keyboard that was found isn't being drawn on, for the status file.
    keyboard_error: Option<String>,
    /// The folder whose first write failed. Finding it again and failing again isn't logged.
    failing: Option<PathBuf>,
    /// Something in the status may have changed; it is written at the end of the step.
    status_changed: bool,
    last_status: Option<Status>,
    /// Something changed that the keyboard doesn't show yet.
    dirty: bool,
    /// A key press lit something the keyboard doesn't show yet. It is drawn at once.
    pressed: bool,
    /// The hardware brightness read 0 (screen saver).
    dimmed: bool,
    last_frame: Option<Instant>,
    next_rescan: Instant,
    next_brightness: Instant,
}

impl<'a> Daemon<'a> {
    fn start(opts: &'a Options) -> io::Result<Daemon<'a>> {
        eprintln!("keyglowd: started, settings in {}", opts.config_dir.display());
        let now = Instant::now();
        let mut watcher = Watcher::new(&opts.config_dir, &opts.input_dir)?;
        let watch_error = watcher.watch_config().err().map(|e| {
            let msg = format!("can't watch {}: {e}", opts.config_dir.display());
            eprintln!("keyglowd: settings changes won't be seen: {msg}");
            msg
        });
        let store = Store::new(opts.config_dir.clone());
        let (setup, settings_error) = match store.load_active() {
            Ok(setup) => (setup, None),
            Err(e) => {
                eprintln!("keyglowd: settings not loaded, using the default: {e}");
                (Setup::owner_default(), Some(e.to_string()))
            }
        };
        // The first step looks for the keyboard and writes the status file.
        Ok(Daemon {
            opts,
            store,
            watcher,
            engine: Engine::new(setup.clone(), seed()),
            setup,
            start: now,
            keyboard: None,
            key_prefix: None,
            keys: Vec::new(),
            keys_complete: false,
            next_key_scan: now,
            watch_error,
            settings_error,
            keyboard_error: None,
            failing: None,
            status_changed: true,
            last_status: None,
            dirty: true,
            pressed: false,
            dimmed: false,
            last_frame: None,
            next_rescan: now,
            next_brightness: now,
        })
    }

    fn run(&mut self, signals: &SignalFd) -> io::Result<()> {
        let mut ready = Vec::new();
        loop {
            let wait = self.step(Instant::now());
            {
                let mut fds = Vec::with_capacity(2 + self.keys.len());
                fds.push(PollFd::new(signals.as_fd(), PollFlags::POLLIN));
                fds.push(PollFd::new(self.watcher.as_fd(), PollFlags::POLLIN));
                fds.extend(self.keys.iter().map(|k| PollFd::new(k.file().as_fd(), PollFlags::POLLIN)));
                match poll(&mut fds, PollTimeout::try_from(timeout_ms(wait)).unwrap_or(PollTimeout::MAX)) {
                    Ok(_) => {}
                    Err(Errno::EINTR) => continue,
                    Err(e) => return Err(e.into()),
                }
                ready.clear();
                ready.extend(fds.iter().map(|f| f.revents().unwrap_or(PollFlags::POLLIN)));
            }
            if !ready[0].is_empty() {
                // SIGTERM or SIGINT. Leave the status file and the keyboard as they are.
                return Ok(());
            }
            // Keys first: a watch event can reopen the key files, and `ready` follows the old list.
            self.on_key_events(&ready[2..], Instant::now());
            if !ready[1].is_empty() {
                self.on_watch_events();
            }
        }
    }

    /// Does whatever is due at `now` (rescan, brightness check, frame) and returns how long
    /// to sleep until the next thing is due, unless an event comes first.
    fn step(&mut self, now: Instant) -> Duration {
        if self.keyboard.is_none() && now >= self.next_rescan {
            self.rescan(now);
        }
        if self.keyboard.is_some() && !self.keys_complete && now >= self.next_key_scan {
            self.scan_keys(now);
        }
        if self.keyboard.is_some() && self.dimmed && now >= self.next_brightness {
            self.check_brightness(now);
        }
        if self.keyboard.is_some() && !self.dimmed && self.wait_at(now).is_zero() {
            // A frame is due. While drawing, look at the brightness every 2 s first.
            if now >= self.next_brightness {
                self.check_brightness(now);
            }
            if !self.dimmed {
                self.draw(now);
            }
        }
        // Once per step, so a keyboard found and lost again in the same step changes nothing.
        if self.status_changed {
            self.publish();
        }
        let now = Instant::now();
        let wait = self.wait_at(now);
        // Woken early by an event: keep the 2 s timers on their own schedule.
        let wait = match (self.keyboard.is_some(), self.dimmed) {
            (false, _) => wait.min(self.next_rescan.saturating_duration_since(now)),
            (true, true) => wait.min(self.next_brightness.saturating_duration_since(now)),
            (true, false) => wait,
        };
        if self.keyboard.is_some() && !self.keys_complete {
            wait.min(self.next_key_scan.saturating_duration_since(now))
        } else {
            wait
        }
    }

    fn wait_at(&self, now: Instant) -> Duration {
        let since = self.last_frame.map_or(Duration::MAX, |last| now.saturating_duration_since(last));
        let moving = self.dirty || self.engine.animating(self.secs(now));
        next_wait(moving, self.pressed, self.keyboard.is_some(), self.dimmed, since, self.frame_interval())
    }

    fn secs(&self, now: Instant) -> f64 {
        now.saturating_duration_since(self.start).as_secs_f64()
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.engine.fps().max(1) as f64)
    }

    fn draw(&mut self, now: Instant) {
        let t = self.secs(now);
        let (rows, cols) = (self.engine.layout().rows, self.engine.layout().cols);
        let bytes = encode(&self.engine.render(t), rows, cols);
        let Some(keyboard) = self.keyboard.as_mut() else { return };
        match keyboard.show(&bytes) {
            Ok(()) => {
                if let Some(dir) = self.failing.take() {
                    eprintln!("keyglowd: keyboard found at {} ({})", dir.display(), keyboard.model().name);
                }
                // A frame drawn while moving isn't the settled look: when the movement stops,
                // one more frame shows how it ends (say, a faded key back to black).
                self.dirty = self.engine.animating(t);
                self.pressed = false;
                // Keep animation frames on a steady beat instead of drifting by the wake-up delay.
                // A frame drawn early for a key press starts the beat again from there.
                let interval = self.frame_interval();
                self.last_frame = Some(match self.last_frame {
                    Some(last) if (interval..interval * 2).contains(&now.saturating_duration_since(last)) => {
                        last + interval
                    }
                    _ => now,
                });
            }
            Err(e) => self.lose(e, now),
        }
    }

    fn check_brightness(&mut self, now: Instant) {
        self.next_brightness = now + BRIGHTNESS_CHECK;
        let Some(keyboard) = self.keyboard.as_mut() else { return };
        // An unreadable brightness counts as lit: a gone keyboard shows up on the next write.
        let off = matches!(keyboard.brightness(), Ok(0));
        if self.dimmed && !off {
            self.dirty = true;
        }
        self.dimmed = off;
    }

    /// Looks for the keyboard and opens it, with its key event files.
    fn rescan(&mut self, now: Instant) {
        self.next_rescan = now + RESCAN;
        self.status_changed = true;
        let Some((dir, model)) = device::find(&self.opts.sysfs_dir) else {
            self.keyboard_error = None;
            self.failing = None;
            return;
        };
        match Keyboard::open(&dir, model) {
            Ok(keyboard) => {
                if self.failing.as_ref() != Some(&dir) {
                    eprintln!("keyglowd: keyboard found at {} ({})", dir.display(), model.name);
                }
                self.engine.set_layout(&model.layout());
                self.key_prefix = input::link_prefix(&dir);
                self.keyboard = Some(keyboard);
                self.keyboard_error = None;
                self.reopen_keys(now);
                self.dirty = true;
                self.dimmed = false;
                self.last_frame = None;
                self.next_brightness = now;
            }
            Err(e) => self.keyboard_error = Some(format!("keyboard at {} can't be opened: {e}", dir.display())),
        }
    }

    fn lose(&mut self, err: io::Error, now: Instant) {
        let Some(keyboard) = self.keyboard.take() else { return };
        let dir = keyboard.dir().to_path_buf();
        // A keyboard that never took a frame is retried quietly until it does.
        let first_write = self.last_frame.is_none();
        if !(first_write && self.failing.as_ref() == Some(&dir)) {
            eprintln!("keyglowd: keyboard lost ({err}), looking for it every {} s", RESCAN.as_secs());
        }
        self.keyboard_error = first_write.then(|| format!("keyboard at {} takes no frames: {err}", dir.display()));
        self.failing = first_write.then_some(dir);
        self.keys.clear();
        self.dimmed = false;
        self.next_rescan = now + RESCAN;
        self.status_changed = true;
    }

    /// Closes the key event files and opens the current ones (a newly found keyboard).
    fn reopen_keys(&mut self, now: Instant) {
        self.keys.clear();
        self.scan_keys(now);
    }

    /// Opens the key event files that aren't open yet, and closes any that are no longer the
    /// keyboard's, after watching their folder again if it went away. Until every one is open
    /// (udev may not have made the links yet, or their access rights), the step retries every 2 s.
    fn scan_keys(&mut self, now: Instant) {
        self.next_key_scan = now + RESCAN;
        self.watcher.watch_inputs();
        let found = input::find(&self.opts.input_dir, self.key_prefix.as_deref());
        // Another Razer keyboard's links stand in until this one's exist (see input::find).
        self.keys.retain(|k| found.iter().any(|p| p == k.path()));
        for path in &found {
            if !self.keys.iter().any(|k| k.path() == path) {
                if let Ok(key) = KeyFile::open(path) {
                    self.keys.push(key);
                }
            }
        }
        self.keys_complete = self.watcher.inputs_watched()
            && !self.keys.is_empty()
            && found.iter().all(|p| self.keys.iter().any(|k| k.path() == p));
    }

    fn on_watch_events(&mut self) {
        let changes = self.watcher.read();
        if changes.settings {
            self.reload();
        }
        if let Some(e) = changes.error {
            eprintln!("keyglowd: settings changes won't be seen: {e}");
            self.watch_error = Some(e);
            self.status_changed = true;
        }
        if changes.inputs && self.keyboard.is_some() {
            self.scan_keys(Instant::now());
        }
    }

    fn reload(&mut self) {
        self.status_changed = true;
        match self.store.load_active() {
            Ok(setup) => {
                if setup != self.setup {
                    self.engine.set_setup(setup.clone());
                    self.setup = setup;
                }
                self.dirty = true;
                self.settings_error = None;
            }
            Err(e) => {
                eprintln!("keyglowd: settings not reloaded, keeping the current ones: {e}");
                self.settings_error = Some(e.to_string());
            }
        }
    }

    /// Reads every key file that poll flagged. Presses only count while the keyboard is lit;
    /// the codes go straight to a grid cell and are dropped.
    fn on_key_events(&mut self, ready: &[PollFlags], now: Instant) {
        let accept = self.keyboard.is_some() && !self.dimmed;
        let t = self.secs(now);
        let broken = PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL;
        let mut index = 0;
        for flags in ready {
            if flags.is_empty() {
                index += 1;
                continue;
            }
            let (codes, open) = match self.keys[index].read() {
                Read::Open(codes) => (codes, !flags.intersects(broken)),
                Read::Gone(codes) => (codes, false),
            };
            if accept {
                for code in codes {
                    if let Some((row, col)) = grid_of_keycode(code) {
                        self.pressed |= self.engine.key_press(row, col, t);
                    }
                }
            }
            if open {
                index += 1;
            } else {
                // Unplugged: the keyboard's folder will be gone too, which the next write shows.
                // Until then (or if it was only this file), look for the links again.
                self.keys.remove(index);
                self.keys_complete = false;
            }
        }
    }

    fn status(&self) -> Status {
        let mut detail = Vec::new();
        if let Some(e) = &self.watch_error {
            detail.push(format!("settings changes not seen: {e}"));
        }
        if let Some(e) = &self.settings_error {
            detail.push(format!("settings not loaded: {e}"));
        }
        match (&self.keyboard, &self.keyboard_error) {
            (Some(k), _) => {
                let untested = if k.model().tested() { "" } else { ", not yet tested with Keyglow" };
                detail.push(format!("{} at {}{untested}", k.model().name, k.dir().display()));
            }
            (None, Some(e)) => detail.push(e.clone()),
            (None, None) => detail.push("keyboard not found".to_string()),
        }
        Status { pid: std::process::id(), keyboard: self.keyboard.is_some(), detail: detail.join("; ") }
    }

    /// Writes the status file when it changed. Failing to write it isn't worth stopping for.
    fn publish(&mut self) {
        self.status_changed = false;
        let status = self.status();
        if self.last_status.as_ref() != Some(&status) {
            let _ = write_status(&self.opts.status_file, &status);
            self.last_status = Some(status);
        }
    }
}

fn seed() -> u64 {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    nanos ^ ((std::process::id() as u64) << 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn next_wait_matches_the_rules() {
        let frame = MS(33);
        // No keyboard: look again in 2 s, whatever else is going on.
        assert_eq!(next_wait(true, false, false, false, MS(0), frame), RESCAN);
        assert_eq!(next_wait(false, false, false, true, MS(9000), frame), RESCAN);
        // Dimmed: check the brightness again in 2 s.
        assert_eq!(next_wait(true, false, true, true, MS(0), frame), BRIGHTNESS_CHECK);
        assert_eq!(next_wait(false, false, true, true, MS(9000), frame), BRIGHTNESS_CHECK);
        // Animating: the time left to the next frame, zero once it is due.
        assert_eq!(next_wait(true, false, true, false, MS(10), frame), MS(23));
        assert_eq!(next_wait(true, false, true, false, MS(33), frame), Duration::ZERO);
        assert_eq!(next_wait(true, false, true, false, MS(500), frame), Duration::ZERO);
        assert_eq!(next_wait(true, false, true, false, Duration::MAX, frame), Duration::ZERO);
        // A key press that lit something: draw now, even mid-frame or when still.
        assert_eq!(next_wait(true, true, true, false, MS(1), frame), Duration::ZERO);
        assert_eq!(next_wait(false, true, true, false, MS(1), frame), Duration::ZERO);
        // ...but not while dimmed or without a keyboard.
        assert_eq!(next_wait(true, true, true, true, MS(1), frame), BRIGHTNESS_CHECK);
        assert_eq!(next_wait(true, true, false, false, MS(1), frame), RESCAN);
        // Still: the time left to the 5 s resend.
        assert_eq!(next_wait(false, false, true, false, MS(1000), frame), MS(4000));
        assert_eq!(next_wait(false, false, true, false, MS(0), frame), RESEND);
        assert_eq!(next_wait(false, false, true, false, MS(5000), frame), Duration::ZERO);
        assert_eq!(next_wait(false, false, true, false, Duration::MAX, frame), Duration::ZERO);
    }

    #[test]
    fn poll_timeouts_round_up() {
        assert_eq!(timeout_ms(Duration::ZERO), 0);
        assert_eq!(timeout_ms(Duration::from_nanos(1)), 1);
        assert_eq!(timeout_ms(Duration::from_micros(33_333)), 34);
        assert_eq!(timeout_ms(MS(2000)), 2000);
        assert_eq!(timeout_ms(Duration::MAX), i32::MAX);
    }
}
