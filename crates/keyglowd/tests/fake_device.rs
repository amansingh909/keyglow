//! Runs the built keyglowd against fake device folders made of regular files in a temp dir.
//! Nothing here touches /sys, /dev/input, the real settings folder or XDG_RUNTIME_DIR.

use keyglow_core::color::Rgb;
use keyglow_core::frame::encode;
use keyglow_core::frame::BLACK_FRAME;
use keyglow_core::store::{read_status, Status};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const FRAME_FILE: &str = "matrix_custom_frame";
const EFFECT_FILE: &str = "matrix_effect_custom";
const BRIGHTNESS_FILE: &str = "matrix_brightness";

struct Daemon {
    child: Child,
    tmp: tempfile::TempDir,
}

impl Daemon {
    /// Starts keyglowd with a settings folder holding one Static layer of `hex`
    /// (or no settings folder at all when `hex` is None) and an empty key event folder.
    fn start(hex: Option<&str>, prepare_sysfs: impl FnOnce(&Path)) -> Daemon {
        Daemon::launch(hex, prepare_sysfs, true)
    }

    /// Like `start`, but the key event folder doesn't exist yet (keyboard unplugged at login).
    fn start_without_key_folder(hex: Option<&str>, prepare_sysfs: impl FnOnce(&Path)) -> Daemon {
        Daemon::launch(hex, prepare_sysfs, false)
    }

    fn launch(hex: Option<&str>, prepare_sysfs: impl FnOnce(&Path), key_folder: bool) -> Daemon {
        let tmp = tempfile::tempdir().unwrap();
        let d = Daemon::paths(tmp.path());
        if let Some(hex) = hex {
            fs::create_dir_all(&d.config).unwrap();
            fs::write(d.config.join("active.json"), static_setup(hex)).unwrap();
        }
        fs::create_dir_all(&d.sysfs).unwrap();
        if key_folder {
            fs::create_dir_all(&d.input).unwrap();
        }
        prepare_sysfs(&d.sysfs);
        let child = Command::new(env!("CARGO_BIN_EXE_keyglowd"))
            .arg("--config-dir")
            .arg(&d.config)
            .arg("--sysfs-dir")
            .arg(&d.sysfs)
            .arg("--input-dir")
            .arg(&d.input)
            .arg("--status-file")
            .arg(&d.status)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Daemon { child, tmp }
    }

    fn paths(root: &Path) -> Paths {
        Paths {
            config: root.join("config").join("keyglow"),
            sysfs: root.join("razerkbd"),
            input: root.join("by-id"),
            status: root.join("run").join("keyglow").join("status.json"),
        }
    }

    fn p(&self) -> Paths {
        Daemon::paths(self.tmp.path())
    }

    fn device(&self, name: &str) -> PathBuf {
        self.p().sysfs.join(name)
    }

    fn status(&self) -> Option<Status> {
        read_status(&self.p().status)
    }

    /// Rewrites active.json the way the settings window does: temp file, then rename.
    fn save(&self, contents: &str) {
        let tmp = self.p().config.join(".active.json.tmp.999");
        fs::write(&tmp, contents).unwrap();
        fs::rename(&tmp, self.p().config.join("active.json")).unwrap();
    }

    /// Sends the signal and returns the exit status and everything written to stderr.
    fn stop(mut self, signal: Signal) -> (ExitStatus, String) {
        kill(Pid::from_raw(self.child.id() as i32), signal).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                break s;
            }
            assert!(Instant::now() < deadline, "keyglowd did not exit");
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut err = String::new();
        self.child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        (status, err)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Paths {
    config: PathBuf,
    sysfs: PathBuf,
    input: PathBuf,
    status: PathBuf,
}

fn static_setup(hex: &str) -> String {
    format!(
        r#"{{"name":"test","fps":30,"brightness":100,"layers":[{{"enabled":true,"opacity":100,"effect":{{"type":"static","color":"{hex}"}}}}]}}"#
    )
}

/// The test keyboard folders are BlackWidow V3 Tenkeyless ones: a 6 by 18 grid.
const ROWS: usize = 6;
const COLS: usize = 18;

/// The encoded frame with every key in one colour.
fn solid(hex: &str) -> Vec<u8> {
    let mut f = BLACK_FRAME;
    for row in f.iter_mut() {
        row.fill(Rgb::from_hex(hex).unwrap());
    }
    encode(&f, ROWS, COLS)
}

/// A lighting interface folder with empty write files and the given brightness.
fn fake_device(sysfs: &Path, name: &str, brightness: &str) -> PathBuf {
    let dir = sysfs.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(FRAME_FILE), "").unwrap();
    fs::write(dir.join(EFFECT_FILE), "").unwrap();
    fs::write(dir.join(BRIGHTNESS_FILE), brightness).unwrap();
    dir
}

/// The other interfaces of the same keyboard, without lighting files.
fn other_interfaces(sysfs: &Path) {
    for name in ["0003:1532:0A24.0014", "0003:1532:0A24.0015", "0003:1532:0A24.0017"] {
        fs::create_dir_all(sysfs.join(name)).unwrap();
    }
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn draws_the_active_setup_and_follows_changes() {
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        other_interfaces(sysfs);
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    let dev = daemon.device("0003:1532:0A24.0016");
    let first = solid("#102030");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == first), "first frame not drawn");
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"1");
    // The status file is written at the end of the same step as the first frame.
    assert!(wait_until(SECOND, || daemon.status().is_some()), "no status file");
    let st = daemon.status().unwrap();
    assert!(st.keyboard, "{st:?}");
    assert_eq!(st.pid, daemon.child.id());

    daemon.save(&static_setup("#405060"));
    let second = solid("#405060");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == second), "frame did not follow the change");
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"1");

    let (exit, err) = daemon.stop(Signal::SIGTERM);
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("started"), "{err}");
    assert!(err.contains("keyboard found"), "{err}");
}

#[test]
fn keeps_the_last_good_setup_when_the_file_is_bad() {
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    let dev = daemon.device("0003:1532:0A24.0016");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == solid("#102030")));

    daemon.save("{ nope");
    assert!(
        wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard && s.detail.contains("settings"))),
        "{:?}",
        daemon.status()
    );
    assert_eq!(read(&dev.join(FRAME_FILE)), solid("#102030"));

    daemon.save(&static_setup("#405060"));
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == solid("#405060")));
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| !s.detail.contains("settings"))));

    let (exit, err) = daemon.stop(Signal::SIGINT);
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("settings"), "{err}");
}

#[test]
fn creates_the_settings_folder_and_starts_from_the_default() {
    let daemon = Daemon::start(None, |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    let dev = daemon.device("0003:1532:0A24.0016");
    // The owner default is animated, so frames keep changing.
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)).len() == 342));
    let a = read(&dev.join(FRAME_FILE));
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) != a), "the default setup should animate");
    assert!(daemon.p().config.is_dir());

    // The folder appeared, so writes to it are seen.
    daemon.save(&static_setup("#405060"));
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == solid("#405060")));
    // The settings file is the window's to write, never ours.
    assert_eq!(fs::read_to_string(daemon.p().config.join("active.json")).unwrap(), static_setup("#405060"));
}

#[test]
fn sleeps_while_still_and_resends_every_five_seconds() {
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    let dev = daemon.device("0003:1532:0A24.0016");
    let frame = solid("#102030");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == frame));
    let cleared = Instant::now();
    fs::write(dev.join(FRAME_FILE), vec![0u8; 342]).unwrap();
    fs::write(dev.join(EFFECT_FILE), "").unwrap();
    // Nothing moves, so nothing is drawn for a while...
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(read(&dev.join(FRAME_FILE)), vec![0u8; 342]);
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"");
    // ...until the 5 s resend wins the keyboard back.
    assert!(wait_until(Duration::from_secs(6), || read(&dev.join(FRAME_FILE)) == frame));
    assert!(cleared.elapsed() >= Duration::from_secs(3), "resent too early: {:?}", cleared.elapsed());
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"1");
}

#[test]
fn draws_nothing_while_the_screen_saver_dims_the_keyboard() {
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "0\n");
    });
    let dev = daemon.device("0003:1532:0A24.0016");
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)));
    std::thread::sleep(SECOND);
    assert_eq!(read(&dev.join(FRAME_FILE)), b"", "drew while dimmed");
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"");
    // The brightness file is only ever read.
    assert_eq!(read(&dev.join(BRIGHTNESS_FILE)), b"0\n");

    fs::write(dev.join(BRIGHTNESS_FILE), "255\n").unwrap();
    assert!(wait_until(Duration::from_secs(3), || read(&dev.join(FRAME_FILE)) == solid("#102030")));
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"1");
    assert_eq!(read(&dev.join(BRIGHTNESS_FILE)), b"255\n");
}

#[test]
fn finds_the_keyboard_again_after_a_write_error_and_a_replug() {
    // The first interface folder's frame file fails every write, like an unplugged keyboard.
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        let dir = fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
        fs::remove_file(dir.join(FRAME_FILE)).unwrap();
        std::os::unix::fs::symlink("/dev/full", dir.join(FRAME_FILE)).unwrap();
    });
    assert!(
        wait_until(SECOND, || daemon.status().is_some_and(|s| !s.keyboard)),
        "{:?}",
        daemon.status()
    );

    // Replugged: the old folder is gone and the lighting interface has a new number.
    fs::remove_dir_all(daemon.device("0003:1532:0A24.0016")).unwrap();
    let dev = fake_device(&daemon.p().sysfs, "0003:1532:0A24.001A", "255\n");
    assert!(wait_until(Duration::from_secs(3), || read(&dev.join(FRAME_FILE)) == solid("#102030")));
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)));

    let (exit, err) = daemon.stop(Signal::SIGTERM);
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard lost"), "{err}");
}

#[test]
fn a_keyboard_that_fails_every_write_is_reported_once() {
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        let dir = fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
        fs::remove_file(dir.join(FRAME_FILE)).unwrap();
        std::os::unix::fs::symlink("/dev/full", dir.join(FRAME_FILE)).unwrap();
    });
    assert!(wait_until(SECOND, || daemon.status().is_some()));
    let written = fs::metadata(&daemon.p().status).unwrap().modified().unwrap();
    // Two more rescans find it and fail again; that is not news.
    let until = Instant::now() + Duration::from_millis(4500);
    while Instant::now() < until {
        let st = daemon.status().unwrap();
        assert!(!st.keyboard, "{st:?}");
        assert!(st.detail.contains("0003:1532:0A24.0016"), "{st:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::metadata(&daemon.p().status).unwrap().modified().unwrap(), written, "status rewritten");
    let (exit, err) = daemon.stop(Signal::SIGTERM);
    assert_eq!(exit.code(), Some(0), "{err}");
    assert_eq!(err.matches("keyboard found").count(), 1, "{err}");
    assert_eq!(err.matches("keyboard lost").count(), 1, "{err}");
}

fn key_event(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut r = vec![0u8; 16];
    r.extend_from_slice(&kind.to_ne_bytes());
    r.extend_from_slice(&code.to_ne_bytes());
    r.extend_from_slice(&value.to_ne_bytes());
    r
}

#[test]
fn a_key_press_lights_its_grid_cell() {
    // A named pipe stands in for the event file: unlike a regular file, poll waits on it.
    let daemon = Daemon::start(None, |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    let link = daemon.p().input.join("usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd");
    assert!(Command::new("mkfifo").arg(&link).status().unwrap().success());
    fs::create_dir_all(&daemon.p().config).unwrap();
    daemon.save(r#"{"name":"keys","layers":[{"effect":{"type":"reactive"}}]}"#);
    let dev = daemon.device("0003:1532:0A24.0016");
    let black = solid("#000000");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == black));
    // The link appeared after the keyboard was found, so only the folder watch can open it.
    use std::io::Write;
    let mut pipe = key_writer(&link, SECOND).expect("keyglowd never opened the new key link");
    // Space goes down (split mid-record across two writes), repeats, and comes back up.
    let mut bytes = key_event(4, 4, 0x2c);
    bytes.extend(key_event(1, 57, 1));
    bytes.extend(key_event(0, 0, 0));
    pipe.write_all(&bytes[..30]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    pipe.write_all(&bytes[30..]).unwrap();
    pipe.write_all(&[key_event(1, 57, 2), key_event(1, 57, 0), key_event(0, 0, 0)].concat()).unwrap();

    // Space is row 5, column 7: bytes 5 * 57 + 3 + 7 * 3 of the frame.
    let space = 5 * 57 + 3 + 7 * 3;
    let lit = |f: &[u8]| f.len() == 342 && f[space..space + 3] != [0, 0, 0];
    assert!(wait_until(SECOND, || lit(&read(&dev.join(FRAME_FILE)))), "space never lit");
    let f = read(&dev.join(FRAME_FILE));
    let others_dark = (0..ROWS * COLS)
        .filter(|&i| i != 5 * COLS + 7)
        .all(|i| f[i / COLS * 57 + 3 + i % COLS * 3..][..3] == [0, 0, 0]);
    assert!(others_dark, "only the pressed key lights up");
    // It fades back to black and the frames stop.
    assert!(wait_until(Duration::from_secs(2), || read(&dev.join(FRAME_FILE)) == black));
    drop(pipe);

    let (exit, err) = daemon.stop(Signal::SIGTERM);
    assert_eq!(exit.code(), Some(0), "{err}");
    // Only the allowed messages, and never a key code.
    for line in err.lines() {
        assert!(
            line.starts_with("keyglowd: started, settings in ") || line.starts_with("keyglowd: keyboard found at "),
            "unexpected message: {line}"
        );
    }
}

const KEY_LINK: &str = "usb-Razer_Razer_BlackWidow_V3_Tenkeyless-event-kbd";
const REACTIVE: &str = r#"{"name":"keys","layers":[{"effect":{"type":"reactive"}}]}"#;
/// Space is row 5, column 7: bytes 5 * 57 + 3 + 7 * 3 of the frame.
const SPACE: usize = 5 * 57 + 3 + 7 * 3;

fn space_lit(frame: &[u8]) -> bool {
    frame.len() == 342 && frame[SPACE..SPACE + 3] != [0, 0, 0]
}

/// A named pipe standing in for a key event file.
fn make_key_link(link: &Path) {
    assert!(Command::new("mkfifo").arg(link).status().unwrap().success());
}

/// The pipe's write end, once keyglowd holds the read end open (it can't be opened before).
/// The first writer that opens is the one kept: closing a pipe's last writer hangs the pipe
/// up, which keyglowd takes for an unplugged keyboard, dropping a press written meanwhile.
fn key_writer(link: &Path, limit: Duration) -> Option<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut writer = None;
    wait_until(limit, || {
        writer = fs::OpenOptions::new().write(true).custom_flags(nix::libc::O_NONBLOCK).open(link).ok();
        writer.is_some()
    });
    writer
}

fn press_space(pipe: &mut fs::File) {
    use std::io::Write;
    pipe.write_all(&[key_event(1, 57, 1), key_event(0, 0, 0), key_event(1, 57, 0), key_event(0, 0, 0)].concat())
        .unwrap();
}

#[test]
fn opens_key_links_whose_folder_appears_later() {
    let daemon = Daemon::start_without_key_folder(None, |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    fs::create_dir_all(&daemon.p().config).unwrap();
    daemon.save(REACTIVE);
    let dev = daemon.device("0003:1532:0A24.0016");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == solid("#000000")));

    // udev makes the folder and the links after the lighting files are already there.
    fs::create_dir(&daemon.p().input).unwrap();
    let link = daemon.p().input.join(KEY_LINK);
    make_key_link(&link);
    let mut pipe = key_writer(&link, Duration::from_secs(3)).expect("keyglowd never opened the key link");
    press_space(&mut pipe);
    assert!(wait_until(SECOND, || space_lit(&read(&dev.join(FRAME_FILE)))), "the press never reached the frame");
}

#[test]
fn opens_key_links_again_after_their_folder_is_recreated() {
    let daemon = Daemon::start(None, |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    fs::create_dir_all(&daemon.p().config).unwrap();
    daemon.save(REACTIVE);
    let dev = daemon.device("0003:1532:0A24.0016");
    // Wait for the reactive setup to show, so the press isn't dropped by the reload.
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == solid("#000000")));
    let link = daemon.p().input.join(KEY_LINK);
    make_key_link(&link);
    let mut pipe = key_writer(&link, Duration::from_secs(3)).expect("keyglowd never opened the key link");
    press_space(&mut pipe);
    assert!(wait_until(SECOND, || space_lit(&read(&dev.join(FRAME_FILE)))));
    assert!(wait_until(Duration::from_secs(2), || read(&dev.join(FRAME_FILE)) == solid("#000000")));

    // The key side goes away (the event file hangs up and udev removes the folder)...
    drop(pipe);
    fs::remove_dir_all(&daemon.p().input).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    // ...and comes back.
    fs::create_dir(&daemon.p().input).unwrap();
    make_key_link(&link);
    let mut pipe = key_writer(&link, Duration::from_secs(3)).expect("keyglowd never reopened the key link");
    press_space(&mut pipe);
    assert!(wait_until(SECOND, || space_lit(&read(&dev.join(FRAME_FILE)))), "the press never reached the frame");
}

#[test]
fn a_key_press_is_drawn_at_once_instead_of_on_the_next_beat() {
    let daemon = Daemon::start(None, |sysfs| {
        fake_device(sysfs, "0003:1532:0A24.0016", "255\n");
    });
    let link = daemon.p().input.join(KEY_LINK);
    make_key_link(&link);
    fs::create_dir_all(&daemon.p().config).unwrap();
    // 10 frames a second, so the next beat is 100 ms away. A fast Spectrum changes colour on
    // every frame and never shows white; the Reactive layer on top lights a pressed key white.
    daemon.save(
        r#"{"name":"keys","fps":10,"layers":[{"effect":{"type":"spectrum","speed":100}},{"effect":{"type":"reactive"}}]}"#,
    );
    let mut pipe = key_writer(&link, Duration::from_secs(3)).expect("keyglowd never opened the key link");
    let dev = daemon.device("0003:1532:0A24.0016");
    let frame = || read(&dev.join(FRAME_FILE));
    // The Spectrum shows: every key the same colour (the default setup at start-up isn't).
    let uniform = |f: &[u8]| f.len() == 342 && (0..ROWS * COLS).all(|i| f[i / COLS * 57 + 3 + i % COLS * 3..][..3] == f[3..6]);
    assert!(wait_until(SECOND, || uniform(&frame())), "the Spectrum setup never showed");

    // Press just after a frame lands, when the next beat is a whole interval away.
    let before = frame();
    let deadline = Instant::now() + SECOND;
    while frame() == before {
        assert!(Instant::now() < deadline, "the frames stopped");
        std::thread::sleep(Duration::from_millis(1));
    }
    let pressed = Instant::now();
    press_space(&mut pipe);
    let white = |f: &[u8]| f.len() == 342 && f[SPACE..SPACE + 3].iter().all(|&b| b >= 200);
    assert!(wait_until(SECOND, || white(&frame())), "the press never reached the frame");
    let took = pressed.elapsed();
    assert!(took < Duration::from_millis(60), "the press waited {took:?} for the next beat");
}

/// A full-size BlackWidow V3: a 6 by 22 grid, so each row is 3 + 22 * 3 bytes.
const FULL_SIZE: &str = "0003:1532:024E.0003";
const FULL_ROW: usize = 3 + 22 * 3;

#[test]
fn draws_other_keyboard_models_on_their_own_grid() {
    let daemon = Daemon::start(Some("#102030"), |sysfs| {
        fake_device(sysfs, FULL_SIZE, "255\n");
    });
    let dev = daemon.device(FULL_SIZE);
    let mut f = BLACK_FRAME;
    for row in f.iter_mut() {
        row.fill(Rgb::from_hex("#102030").unwrap());
    }
    let full = encode(&f, 6, 22);
    assert_eq!(full.len(), 6 * FULL_ROW);
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == full), "no 6 by 22 frame");
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)), "no status file");
    let detail = daemon.status().unwrap().detail;
    assert!(detail.contains("Razer BlackWidow V3 at "), "{detail}");
    assert!(detail.contains("not yet tested"), "{detail}");

    let (exit, err) = daemon.stop(Signal::SIGTERM);
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard found at ") && err.contains("(Razer BlackWidow V3)"), "{err}");
}

#[test]
fn a_number_pad_key_lights_on_a_full_size_keyboard() {
    let daemon = Daemon::start(None, |sysfs| {
        fake_device(sysfs, FULL_SIZE, "255\n");
    });
    let link = daemon.p().input.join("usb-Razer_Razer_BlackWidow_V3-event-kbd");
    make_key_link(&link);
    fs::create_dir_all(&daemon.p().config).unwrap();
    daemon.save(REACTIVE);
    let dev = daemon.device(FULL_SIZE);
    let dark = |f: &[u8]| f.len() == 6 * FULL_ROW && (0..6).all(|r| f[r * FULL_ROW + 3..(r + 1) * FULL_ROW].iter().all(|&b| b == 0));
    assert!(wait_until(SECOND, || dark(&read(&dev.join(FRAME_FILE)))), "the reactive setup never showed");
    let mut pipe = key_writer(&link, Duration::from_secs(3)).expect("keyglowd never opened the key link");
    // Keypad Enter is row 4, column 21.
    use std::io::Write;
    pipe.write_all(&[key_event(1, 96, 1), key_event(0, 0, 0), key_event(1, 96, 0), key_event(0, 0, 0)].concat()).unwrap();
    let enter = 4 * FULL_ROW + 3 + 21 * 3;
    assert!(
        wait_until(SECOND, || read(&dev.join(FRAME_FILE)).get(enter..enter + 3).is_some_and(|c| c != [0, 0, 0])),
        "keypad Enter never lit"
    );
}

#[test]
fn waits_for_a_keyboard_that_is_not_there_yet() {
    let daemon = Daemon::start(Some("#102030"), |_| {});
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| !s.keyboard)));
    let dev = fake_device(&daemon.p().sysfs, "0003:1532:0A24.0016", "255\n");
    assert!(wait_until(Duration::from_secs(3), || read(&dev.join(FRAME_FILE)) == solid("#102030")));
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)));
}
