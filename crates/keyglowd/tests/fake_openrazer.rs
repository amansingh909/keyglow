//! Runs the built keyglowd against a fake session bus with a fake OpenRazer daemon on it, plus
//! fake device folders. The fake bus speaks just enough D-Bus for keyglowd, written apart from
//! keyglowd's own code so the two can't share a mistake. Nothing here touches the real bus.

use keyglow_core::color::Rgb;
use keyglow_core::frame::{encode, BLACK_FRAME};
use keyglow_core::store::{read_status, Status};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FRAME_FILE: &str = "matrix_custom_frame";
const EFFECT_FILE: &str = "matrix_effect_custom";
const BRIGHTNESS_FILE: &str = "matrix_brightness";
const TKL: u16 = 0x0A24;
const SERIAL: &str = "IO2335F55401610";
const SECOND: Duration = Duration::from_secs(1);

/// What the fake daemon was asked, and how it answers.
#[derive(Default)]
struct State {
    /// The OpenRazer daemon is on the bus.
    running: bool,
    /// Serial numbers and USB product IDs of the devices it serves.
    devices: Vec<(String, u16)>,
    /// `setKeyRow` payloads, with the object path they went to.
    frames: Vec<(String, Vec<u8>)>,
    /// `setCustom` calls, by object path.
    customs: Vec<String>,
    /// A call to the daemon left out NO_AUTO_START, which would start it if it isn't running.
    auto_start: bool,
    /// The daemon holds its name but never answers, like one that is stuck.
    silent: bool,
    /// `setKeyRow` and `setCustom` answer with an error.
    refuse_frames: bool,
    /// The daemon's effect sync is on.
    sync: bool,
    /// Connections made to the bus so far.
    connections: usize,
    /// Messages that weren't what a well-behaved client sends.
    faults: Vec<String>,
}

struct FakeBus {
    state: Arc<Mutex<State>>,
    address: String,
}

impl FakeBus {
    fn start(dir: &Path, running: bool, devices: &[(&str, u16)]) -> FakeBus {
        let socket = dir.join("bus");
        let listener = UnixListener::bind(&socket).unwrap();
        let state = Arc::new(Mutex::new(State {
            running,
            devices: devices.iter().map(|(s, p)| (s.to_string(), *p)).collect(),
            ..State::default()
        }));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for (n, stream) in listener.incoming().enumerate() {
                let Ok(stream) = stream else { return };
                let state = Arc::clone(&shared);
                std::thread::spawn(move || serve(stream, &state, n + 1));
            }
        });
        FakeBus { state, address: format!("unix:path={}", socket.display()) }
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.state.lock().unwrap())
    }

    fn last_frame(&self) -> Option<(String, Vec<u8>)> {
        self.with(|s| s.frames.last().cloned())
    }
}

/// One client connection: the login, then messages until it hangs up.
fn serve(mut stream: UnixStream, state: &Mutex<State>, n: usize) {
    state.lock().unwrap().connections += 1;
    let mut buf = Vec::new();
    let Some(auth) = line(&mut stream, &mut buf) else { return };
    if !auth.starts_with(b"\0AUTH EXTERNAL ") {
        state.lock().unwrap().faults.push(format!("login: {:?}", String::from_utf8_lossy(&auth)));
        return;
    }
    stream.write_all(b"OK 0123456789abcdef0123456789abcdef\r\n").unwrap();
    match line(&mut stream, &mut buf) {
        Some(l) if l == b"BEGIN" => {}
        other => {
            state.lock().unwrap().faults.push(format!("expected BEGIN, got {other:?}"));
            return;
        }
    }
    let mut serial = 0;
    let mut said_hello = false;
    loop {
        while let Some((msg, len)) = parse(&buf) {
            buf.drain(..len);
            serial += 1;
            let Some(reply) = answer(&msg, state, n, &mut said_hello) else { continue };
            if stream.write_all(&reply.encode(serial, msg.serial)).is_err() {
                return;
            }
            // The real bus follows the Hello reply with a NameAcquired signal.
            if msg.member == "Hello" {
                serial += 1;
                if stream.write_all(&name_acquired(serial, &format!(":1.{n}"))).is_err() {
                    return;
                }
            }
        }
        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(k) => buf.extend_from_slice(&chunk[..k]),
        }
    }
}

/// Reads one `\r\n`-ended login line into `buf` and returns it without the ending.
fn line(stream: &mut UnixStream, buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    loop {
        if let Some(i) = buf.windows(2).position(|w| w == b"\r\n") {
            let l = buf[..i].to_vec();
            buf.drain(..i + 2);
            return Some(l);
        }
        let mut chunk = [0u8; 256];
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(k) => buf.extend_from_slice(&chunk[..k]),
        }
    }
}

#[derive(Debug, Default)]
struct Msg {
    kind: u8,
    flags: u8,
    serial: u32,
    path: String,
    interface: String,
    member: String,
    destination: String,
    signature: String,
    body: Vec<u8>,
}

const NO_AUTO_START: u8 = 0x2;

fn align(n: usize, to: usize) -> usize {
    n.div_ceil(to) * to
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

/// A whole little-endian message at the start of `b` and its length, once it has all arrived.
fn parse(b: &[u8]) -> Option<(Msg, usize)> {
    if b.len() < 16 {
        return None;
    }
    assert_eq!(b[0], b'l', "keyglowd writes little-endian messages");
    assert_eq!(b[3], 1, "protocol version");
    let body_len = u32_at(b, 4) as usize;
    let fields_end = 16 + u32_at(b, 12) as usize;
    let total = align(fields_end, 8) + body_len;
    if b.len() < total {
        return None;
    }
    let mut m = Msg { kind: b[1], flags: b[2], serial: u32_at(b, 8), ..Msg::default() };
    let mut i = 16;
    while i < fields_end {
        i = align(i, 8);
        let code = b[i];
        let sig_len = b[i + 1] as usize;
        let sig = std::str::from_utf8(&b[i + 2..i + 2 + sig_len]).unwrap().to_string();
        i += 3 + sig_len;
        let text = match sig.as_str() {
            "s" | "o" => {
                i = align(i, 4);
                let len = u32_at(b, i) as usize;
                let s = std::str::from_utf8(&b[i + 4..i + 4 + len]).unwrap().to_string();
                assert_eq!(b[i + 4 + len], 0, "strings end in a nul byte");
                i += 5 + len;
                s
            }
            "g" => {
                let len = b[i] as usize;
                let s = std::str::from_utf8(&b[i + 1..i + 1 + len]).unwrap().to_string();
                i += 2 + len;
                s
            }
            "u" => {
                i = align(i, 4) + 4;
                String::new()
            }
            other => panic!("unexpected header field type {other}"),
        };
        match code {
            1 => m.path = text,
            2 => m.interface = text,
            3 => m.member = text,
            6 => m.destination = text,
            8 => m.signature = text,
            _ => {}
        }
    }
    m.body = b[align(fields_end, 8)..total].to_vec();
    Some((m, total))
}

/// A reply body, encoded from offset 0 (bodies start 8-aligned).
enum Body {
    Empty,
    Bool(bool),
    Str(String),
    Strs(Vec<String>),
    Ints(Vec<i32>),
}

/// The signal the bus sends a client once it has its unique name.
fn name_acquired(serial: u32, name: &str) -> Vec<u8> {
    let mut m = vec![b'l', 4, 1, 1, 0, 0, 0, 0];
    m.extend_from_slice(&serial.to_le_bytes());
    m.extend_from_slice(&[0; 4]);
    let field = |m: &mut Vec<u8>, code: u8, sig: u8, s: &str| {
        m.resize(align(m.len(), 8), 0);
        m.extend_from_slice(&[code, 1, sig, 0]);
        m.extend_from_slice(&(s.len() as u32).to_le_bytes());
        m.extend_from_slice(s.as_bytes());
        m.push(0);
    };
    field(&mut m, 1, b'o', "/org/freedesktop/DBus");
    field(&mut m, 2, b's', "org.freedesktop.DBus");
    field(&mut m, 3, b's', "NameAcquired");
    m.resize(align(m.len(), 8), 0);
    m.extend_from_slice(&[8, 1, b'g', 0, 1, b's', 0]);
    let fields_len = (m.len() - 16) as u32;
    m[12..16].copy_from_slice(&fields_len.to_le_bytes());
    m.resize(align(m.len(), 8), 0);
    let mut body = (name.len() as u32).to_le_bytes().to_vec();
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    m[4..8].copy_from_slice(&(body.len() as u32).to_le_bytes());
    m.extend_from_slice(&body);
    m
}

struct Reply {
    error: Option<String>,
    signature: &'static str,
    body: Vec<u8>,
}

impl Reply {
    fn ok(body: Body) -> Reply {
        let mut b = Vec::new();
        let put_str = |b: &mut Vec<u8>, s: &str| {
            b.resize(align(b.len(), 4), 0);
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        };
        let signature = match body {
            Body::Empty => "",
            Body::Bool(v) => {
                b.extend_from_slice(&u32::from(v).to_le_bytes());
                "b"
            }
            Body::Str(s) => {
                put_str(&mut b, &s);
                "s"
            }
            Body::Strs(list) => {
                b.extend_from_slice(&[0; 4]);
                for s in &list {
                    put_str(&mut b, s);
                }
                let len = (b.len() - 4) as u32;
                b[..4].copy_from_slice(&len.to_le_bytes());
                "as"
            }
            Body::Ints(list) => {
                b.extend_from_slice(&((list.len() * 4) as u32).to_le_bytes());
                for v in list {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                "ai"
            }
        };
        Reply { error: None, signature, body: b }
    }

    fn error(name: &str, text: &str) -> Reply {
        let mut r = Reply::ok(Body::Str(text.to_string()));
        r.error = Some(name.to_string());
        r
    }

    fn encode(&self, serial: u32, reply_to: u32) -> Vec<u8> {
        let mut m = vec![b'l', if self.error.is_some() { 3 } else { 2 }, 1, 1];
        m.extend_from_slice(&(self.body.len() as u32).to_le_bytes());
        m.extend_from_slice(&serial.to_le_bytes());
        m.extend_from_slice(&[0; 4]);
        let field = |m: &mut Vec<u8>, code: u8, sig: &str| {
            m.resize(align(m.len(), 8), 0);
            m.extend_from_slice(&[code, 1, sig.as_bytes()[0], 0]);
        };
        let string = |m: &mut Vec<u8>, s: &str| {
            m.resize(align(m.len(), 4), 0);
            m.extend_from_slice(&(s.len() as u32).to_le_bytes());
            m.extend_from_slice(s.as_bytes());
            m.push(0);
        };
        field(&mut m, 5, "u");
        m.extend_from_slice(&reply_to.to_le_bytes());
        field(&mut m, 7, "s");
        string(&mut m, "org.razer");
        if let Some(name) = &self.error {
            field(&mut m, 4, "s");
            string(&mut m, name);
        }
        if !self.signature.is_empty() {
            field(&mut m, 8, "g");
            m.push(self.signature.len() as u8);
            m.extend_from_slice(self.signature.as_bytes());
            m.push(0);
        }
        let fields_len = (m.len() - 16) as u32;
        m[12..16].copy_from_slice(&fields_len.to_le_bytes());
        m.resize(align(m.len(), 8), 0);
        m.extend_from_slice(&self.body);
        m
    }
}

/// The reply to a call, or None for no reply at all.
fn answer(msg: &Msg, state: &Mutex<State>, n: usize, said_hello: &mut bool) -> Option<Reply> {
    let mut s = state.lock().unwrap();
    if msg.kind != 1 {
        s.faults.push(format!("message of type {} sent", msg.kind));
    }
    if msg.destination == "org.freedesktop.DBus" && msg.member == "Hello" {
        *said_hello = true;
        return Some(Reply::ok(Body::Str(format!(":1.{n}"))));
    }
    if !*said_hello {
        s.faults.push(format!("{} called before Hello", msg.member));
    }
    if msg.destination != "org.razer" {
        s.faults.push(format!("call to {}", msg.destination));
        return Some(Reply::error("org.freedesktop.DBus.Error.ServiceUnknown", "unknown name"));
    }
    if msg.flags & NO_AUTO_START == 0 {
        s.auto_start = true;
    }
    if !s.running {
        return Some(Reply::error("org.freedesktop.DBus.Error.NameHasNoOwner", "org.razer is not running"));
    }
    if s.silent {
        return None;
    }
    let device = msg.path.strip_prefix("/org/razer/device/").and_then(|serial| s.devices.iter().find(|d| d.0 == serial)).cloned();
    if s.refuse_frames && matches!(msg.member.as_str(), "setKeyRow" | "setCustom") && device.is_some() {
        return Some(Reply::error("org.freedesktop.DBus.Python.OSError", "the keyboard said no"));
    }
    Some(match (msg.path.as_str(), msg.interface.as_str(), msg.member.as_str(), device) {
        ("/org/razer", "razer.devices", "getDevices", _) => Reply::ok(Body::Strs(s.devices.iter().map(|d| d.0.clone()).collect())),
        ("/org/razer", "razer.devices", "getSyncEffects", _) => Reply::ok(Body::Bool(s.sync)),
        (_, "razer.device.misc", "getVidPid", Some((_, pid))) => Reply::ok(Body::Ints(vec![0x1532, pid as i32])),
        (path, "razer.device.lighting.chroma", "setKeyRow", Some(_)) if msg.signature == "ay" => {
            let len = u32_at(&msg.body, 0) as usize;
            s.frames.push((path.to_string(), msg.body[4..4 + len].to_vec()));
            Reply::ok(Body::Empty)
        }
        (path, "razer.device.lighting.chroma", "setCustom", Some(_)) if msg.signature.is_empty() => {
            s.customs.push(path.to_string());
            Reply::ok(Body::Empty)
        }
        (_, _, _, None) if msg.path != "/org/razer" => {
            Reply::error("org.freedesktop.DBus.Error.UnknownObject", "no such device")
        }
        _ => {
            s.faults.push(format!("unexpected call {} {}.{} ({})", msg.path, msg.interface, msg.member, msg.signature));
            Reply::error("org.freedesktop.DBus.Error.UnknownMethod", "no such method")
        }
    })
}

struct Daemon {
    child: Child,
    tmp: tempfile::TempDir,
}

impl Daemon {
    /// Starts keyglowd with one Static layer of `hex` (or an animated Spectrum when `hex` is
    /// None), the given bus address, and an empty key event folder.
    fn start(tmp: tempfile::TempDir, hex: Option<&str>, bus: &str) -> Daemon {
        let root = tmp.path();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("razerkbd")).unwrap();
        fs::create_dir_all(root.join("by-id")).unwrap();
        let setup = match hex {
            Some(hex) => format!(
                r#"{{"name":"test","fps":30,"layers":[{{"effect":{{"type":"static","color":"{hex}"}}}}]}}"#
            ),
            None => r#"{"name":"test","fps":30,"layers":[{"effect":{"type":"spectrum","speed":100}}]}"#.to_string(),
        };
        fs::write(root.join("config").join("active.json"), setup).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_keyglowd"))
            .arg("--config-dir")
            .arg(root.join("config"))
            .arg("--sysfs-dir")
            .arg(root.join("razerkbd"))
            .arg("--input-dir")
            .arg(root.join("by-id"))
            .arg("--status-file")
            .arg(root.join("status.json"))
            .arg("--bus-address")
            .arg(bus)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Daemon { child, tmp }
    }

    fn status(&self) -> Option<Status> {
        read_status(&self.tmp.path().join("status.json"))
    }

    fn stop(mut self) -> (ExitStatus, String) {
        kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
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

/// A lighting interface folder of the tested keyboard, with empty write files.
fn lighting_folder(root: &Path, brightness: &str) -> PathBuf {
    let dir = root.join("razerkbd").join("0003:1532:0A24.0016");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(FRAME_FILE), "").unwrap();
    fs::write(dir.join(EFFECT_FILE), "").unwrap();
    fs::write(dir.join(BRIGHTNESS_FILE), brightness).unwrap();
    dir
}

fn solid(hex: &str) -> Vec<u8> {
    let mut f = BLACK_FRAME;
    for row in f.iter_mut() {
        row.fill(Rgb::from_hex(hex).unwrap());
    }
    encode(&f, 6, 18)
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

const DEVICE_PATH: &str = "/org/razer/device/IO2335F55401610";

#[test]
fn frames_go_through_the_openrazer_daemon_when_it_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "255\n");
    // A mouse comes first in the daemon's list; it isn't a keyboard Keyglow knows.
    let bus = FakeBus::start(tmp.path(), true, &[("PM1234", 0x0084), (SERIAL, TKL)]);
    let daemon = Daemon::start(tmp, Some("#102030"), &bus.address);

    let want = solid("#102030");
    assert!(
        wait_until(SECOND, || bus.last_frame() == Some((DEVICE_PATH.to_string(), want.clone()))),
        "no frame reached the daemon: {:?}",
        bus.with(|s| s.faults.clone())
    );
    assert!(wait_until(SECOND, || bus.with(|s| s.customs.contains(&DEVICE_PATH.to_string()))), "setCustom never called");
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)), "{:?}", daemon.status());
    let detail = daemon.status().unwrap().detail;
    assert!(detail.contains("Razer BlackWidow V3 Tenkeyless through the OpenRazer daemon"), "{detail}");
    // The driver's files are left alone while the daemon takes the frames.
    assert_eq!(read(&dev.join(FRAME_FILE)), b"");
    assert_eq!(read(&dev.join(EFFECT_FILE)), b"");

    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard found through the OpenRazer daemon"), "{err}");
    bus.with(|s| {
        assert!(s.faults.is_empty(), "{:?}", s.faults);
        assert!(!s.auto_start, "a call could have started the daemon");
    });
}

#[test]
fn needs_none_of_the_driver_files_when_the_daemon_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    let daemon = Daemon::start(tmp, Some("#405060"), &bus.address);
    let want = solid("#405060");
    assert!(wait_until(SECOND, || bus.last_frame() == Some((DEVICE_PATH.to_string(), want.clone()))));
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)), "{:?}", daemon.status());
    bus.with(|s| assert!(s.faults.is_empty(), "{:?}", s.faults));
}

#[test]
fn uses_the_driver_files_until_the_daemon_starts() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "255\n");
    let bus = FakeBus::start(tmp.path(), false, &[(SERIAL, TKL)]);
    let daemon = Daemon::start(tmp, Some("#102030"), &bus.address);
    let want = solid("#102030");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == want), "the files weren't used");
    assert!(wait_until(SECOND, || read(&dev.join(EFFECT_FILE)) == b"1"));
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.detail.contains(" at "))), "{:?}", daemon.status());
    assert_eq!(bus.last_frame(), None);

    bus.with(|s| s.running = true);
    assert!(
        wait_until(Duration::from_secs(3), || bus.last_frame() == Some((DEVICE_PATH.to_string(), want.clone()))),
        "never moved over to the daemon"
    );
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.detail.contains("through the OpenRazer daemon"))));
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard found at "), "{err}");
    assert!(err.contains("keyboard found through the OpenRazer daemon"), "{err}");
    bus.with(|s| {
        assert!(s.faults.is_empty(), "{:?}", s.faults);
        assert!(!s.auto_start, "a call could have started the daemon");
    });
}

#[test]
fn goes_back_to_the_driver_files_when_the_daemon_stops() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "255\n");
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    // An animated setup, so frames keep going out.
    let daemon = Daemon::start(tmp, None, &bus.address);
    assert!(wait_until(SECOND, || bus.with(|s| s.frames.len() >= 3)), "no frames reached the daemon");

    bus.with(|s| s.running = false);
    assert!(wait_until(Duration::from_secs(4), || read(&dev.join(FRAME_FILE)).len() == 342), "never fell back to the files");
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard && s.detail.contains(" at "))));
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard lost"), "{err}");
    bus.with(|s| assert!(s.faults.is_empty(), "{:?}", s.faults));
}

#[test]
fn draws_nothing_through_the_daemon_while_the_keyboard_is_dimmed() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "0\n");
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    let daemon = Daemon::start(tmp, Some("#102030"), &bus.address);
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)));
    std::thread::sleep(SECOND);
    assert_eq!(bus.last_frame(), None, "drew while dimmed");

    fs::write(dev.join(BRIGHTNESS_FILE), "255\n").unwrap();
    let want = solid("#102030");
    assert!(wait_until(Duration::from_secs(3), || bus.last_frame() == Some((DEVICE_PATH.to_string(), want.clone()))));
    assert_eq!(read(&dev.join(FRAME_FILE)), b"");
}

#[test]
fn works_without_a_session_bus() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "255\n");
    let missing = format!("unix:path={}", tmp.path().join("no-bus-here").display());
    let daemon = Daemon::start(tmp, Some("#102030"), &missing);
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == solid("#102030")));
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)));
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard found at "), "{err}");
}

/// The longest stretch without a new frame in the folder over `span`, for an animated setup.
fn longest_gap(frame: &Path, span: Duration) -> Duration {
    let start = Instant::now();
    let (mut last, mut seen, mut worst) = (read(frame), Instant::now(), Duration::ZERO);
    while start.elapsed() < span {
        std::thread::sleep(Duration::from_millis(5));
        let now = read(frame);
        if now != last {
            worst = worst.max(seen.elapsed());
            (last, seen) = (now, Instant::now());
        }
    }
    worst.max(seen.elapsed())
}

#[test]
fn a_daemon_that_never_answers_does_not_stall_the_lighting() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "255\n");
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    bus.with(|s| s.silent = true);
    let daemon = Daemon::start(tmp, None, &bus.address);
    assert!(wait_until(Duration::from_secs(2), || read(&dev.join(FRAME_FILE)).len() == 342), "the files weren't used");
    // Asking the stuck daemon again holds the frames up a little, and less and less often.
    let gap = longest_gap(&dev.join(FRAME_FILE), Duration::from_secs(7));
    assert!(gap < Duration::from_millis(700), "the frames stopped for {gap:?}");
    let tries = bus.with(|s| s.connections);
    assert!(tries <= 4, "asked the stuck daemon {tries} times in about 9 s");
    let detail = daemon.status().unwrap().detail;
    assert!(detail.contains("OpenRazer daemon not used: "), "{detail}");
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert_eq!(err.matches("can't use the OpenRazer daemon").count(), 1, "{err}");
}

#[test]
fn a_daemon_that_refuses_frames_leaves_them_to_the_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = lighting_folder(tmp.path(), "255\n");
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    bus.with(|s| s.refuse_frames = true);
    let daemon = Daemon::start(tmp, Some("#102030"), &bus.address);
    let want = solid("#102030");
    assert!(wait_until(SECOND, || read(&dev.join(FRAME_FILE)) == want), "never fell back to the files");
    assert!(
        wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard && s.detail.contains("OpenRazer daemon not used: "))),
        "{:?}",
        daemon.status()
    );

    // Once it takes frames again, they move back to it.
    bus.with(|s| s.refuse_frames = false);
    assert!(
        wait_until(Duration::from_secs(5), || bus.last_frame() == Some((DEVICE_PATH.to_string(), want.clone()))),
        "never went back to the daemon"
    );
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.detail.contains("through the OpenRazer daemon")
        && !s.detail.contains("not used"))));
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert_eq!(err.matches("can't use the OpenRazer daemon").count(), 1, "{err}");
    assert!(!err.contains("keyboard lost"), "{err}");
    bus.with(|s| assert!(s.faults.is_empty(), "{:?}", s.faults));
}

#[test]
fn finds_the_daemons_keyboard_again_after_a_replug() {
    let tmp = tempfile::tempdir().unwrap();
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    let daemon = Daemon::start(tmp, None, &bus.address);
    assert!(wait_until(SECOND, || bus.with(|s| s.frames.len() >= 3)), "no frames reached the daemon");
    // Unplugged: the daemon forgets the device, then finds it again.
    let devices = bus.with(|s| std::mem::take(&mut s.devices));
    assert!(wait_until(Duration::from_secs(2), || daemon.status().is_some_and(|s| !s.keyboard)), "{:?}", daemon.status());
    let before = bus.with(|s| s.frames.len());
    bus.with(|s| s.devices = devices);
    assert!(wait_until(Duration::from_secs(3), || bus.with(|s| s.frames.len() > before + 3)), "never found it again");
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert!(err.contains("keyboard lost"), "{err}");
    assert_eq!(err.matches("keyboard found through the OpenRazer daemon").count(), 2, "{err}");
}

#[test]
fn says_so_when_the_daemon_syncs_effects_to_other_devices() {
    let tmp = tempfile::tempdir().unwrap();
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    bus.with(|s| s.sync = true);
    let daemon = Daemon::start(tmp, Some("#102030"), &bus.address);
    assert!(wait_until(SECOND, || daemon.status().is_some_and(|s| s.keyboard)));
    let detail = daemon.status().unwrap().detail;
    assert!(detail.contains("effect sync"), "{detail}");
    let (exit, err) = daemon.stop();
    assert_eq!(exit.code(), Some(0), "{err}");
    assert_eq!(err.lines().filter(|l| l.contains("effect sync")).count(), 1, "{err}");
}

#[test]
fn follows_the_keyboard_folder_after_a_replug_only_the_daemon_saw() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let old = lighting_folder(&root, "255\n");
    let bus = FakeBus::start(tmp.path(), true, &[(SERIAL, TKL)]);
    let daemon = Daemon::start(tmp, Some("#102030"), &bus.address);
    assert!(wait_until(SECOND, || bus.last_frame().is_some()));
    // The keyboard comes back under a new folder number before the next frame, and the screen
    // saver has it dimmed.
    fs::remove_dir_all(&old).unwrap();
    let new = root.join("razerkbd").join("0003:1532:0A24.001A");
    fs::create_dir_all(&new).unwrap();
    fs::write(new.join(BRIGHTNESS_FILE), "0\n").unwrap();
    assert!(
        wait_until(Duration::from_secs(3), || daemon.status().is_some_and(|s| s.keyboard)) && {
            std::thread::sleep(Duration::from_millis(2500));
            true
        }
    );
    let frames = bus.with(|s| s.frames.len());
    // The 5 s resend comes and goes without a frame while dimmed...
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(bus.with(|s| s.frames.len()), frames, "drew while the new folder said dimmed");
    // ...and the lighting comes back with the brightness.
    fs::write(new.join(BRIGHTNESS_FILE), "255\n").unwrap();
    assert!(wait_until(Duration::from_secs(3), || bus.with(|s| s.frames.len()) > frames));
}
