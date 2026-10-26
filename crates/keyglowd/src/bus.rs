//! A small client for the D-Bus session bus: just enough of the protocol to call a few methods
//! and read their replies, so keyglowd needs no D-Bus library and no thread of its own for it.
//! It writes little-endian messages and reads either byte order.

use nix::libc;
use std::io::{self, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long a frame's calls may wait for their replies.
pub const TIMEOUT: Duration = Duration::from_secs(1);

const METHOD_CALL: u8 = 1;
const METHOD_RETURN: u8 = 2;
const ERROR: u8 = 3;
/// Every call carries this flag: keyglowd never wants a call to start a program that isn't
/// running (the OpenRazer daemon can be started that way).
const NO_AUTO_START: u8 = 0x2;
/// The largest message taken in. Replies to keyglowd are tiny; the protocol allows 128 MiB,
/// more than the service may use.
const MAX_MESSAGE: usize = 1 << 20;

const PATH: u8 = 1;
const INTERFACE: u8 = 2;
const MEMBER: u8 = 3;
const ERROR_NAME: u8 = 4;
const REPLY_SERIAL: u8 = 5;
const DESTINATION: u8 = 6;
const SIGNATURE: u8 = 8;

/// A method call.
pub struct Call<'a> {
    pub destination: &'a str,
    pub path: &'a str,
    pub interface: &'a str,
    pub member: &'a str,
    pub args: Args<'a>,
}

/// A call's arguments: the few kinds keyglowd sends.
pub enum Args<'a> {
    None,
    Bytes(&'a [u8]),
}

/// A method's return values.
#[derive(Debug)]
pub struct Reply {
    signature: String,
    body: Vec<u8>,
    big_endian: bool,
}

/// An error reply, kept inside the `io::Error` a call returns.
#[derive(Debug)]
pub struct Refused {
    pub name: String,
    pub message: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}: {}", self.name, self.message)
        }
    }
}

impl std::error::Error for Refused {}

/// The error reply a call failed with, if it got one.
pub fn refused(e: &io::Error) -> Option<&Refused> {
    e.get_ref()?.downcast_ref::<Refused>()
}

/// An open, logged-in connection to a bus.
pub struct Bus {
    stream: UnixStream,
    serial: u32,
    /// Bytes read past the end of the last whole message.
    inbox: Vec<u8>,
    /// Calls sent whose replies haven't been collected yet.
    waiting: Vec<u32>,
    /// Replies to those that came in while another one was awaited.
    early: Vec<(u32, Message)>,
    timeout: Duration,
}

impl Bus {
    /// Connects to the first Unix socket in a D-Bus `address`, logs in as this user and says
    /// Hello, as every client must before anything else.
    pub fn connect(address: &str, timeout: Duration) -> io::Result<Bus> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("no Unix socket in the bus address {address:?}"));
        for socket in sockets(address) {
            let stream = match &socket {
                Socket::Path(path) => UnixStream::connect(path),
                Socket::Abstract(name) => SocketAddr::from_abstract_name(name).and_then(|a| UnixStream::connect_addr(&a)),
            };
            match stream {
                Ok(stream) => return Bus::login(stream, timeout),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn login(mut stream: UnixStream, timeout: Duration) -> io::Result<Bus> {
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        // SAFETY: getuid has no preconditions and can't fail.
        let uid = unsafe { libc::getuid() };
        stream.write_all(format!("\0AUTH EXTERNAL {}\r\n", uid_hex(uid)).as_bytes())?;
        let answer = read_line(&mut stream).map_err(|e| timed_out(e, "the bus didn't answer the login"))?;
        if !answer.starts_with("OK ") {
            return Err(io::Error::other(format!("the bus refused the login ({answer})")));
        }
        // The messages may follow BEGIN at once.
        stream.write_all(b"BEGIN\r\n")?;
        let mut bus = Bus { stream, serial: 0, inbox: Vec::new(), waiting: Vec::new(), early: Vec::new(), timeout };
        bus.call(&Call {
            destination: "org.freedesktop.DBus",
            path: "/org/freedesktop/DBus",
            interface: "org.freedesktop.DBus",
            member: "Hello",
            args: Args::None,
        })?;
        Ok(bus)
    }

    /// How long [`Bus::reply`] waits from now on.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Sends a call and returns its serial number, to wait for its reply with [`Bus::reply`].
    pub fn send(&mut self, call: &Call) -> io::Result<u32> {
        // Serial numbers are never 0.
        self.serial = self.serial.checked_add(1).unwrap_or(1);
        self.stream.write_all(&encode(call, self.serial)).map_err(|e| timed_out(e, "the bus took no message"))?;
        self.waiting.push(self.serial);
        Ok(self.serial)
    }

    /// Waits for the reply to the call with this serial number. Replies to other calls still
    /// waiting are kept for them; anything else is dropped.
    pub fn reply(&mut self, serial: u32) -> io::Result<Reply> {
        let reply = self.wait_for(serial);
        self.waiting.retain(|&s| s != serial);
        reply
    }

    fn wait_for(&mut self, serial: u32) -> io::Result<Reply> {
        let deadline = Instant::now() + self.timeout;
        let message = loop {
            if let Some(i) = self.early.iter().position(|(s, _)| *s == serial) {
                break self.early.remove(i).1;
            }
            if let Some((m, len)) = decode(&self.inbox)? {
                self.inbox.drain(..len);
                let Some(to) = m.reply_serial.filter(|_| matches!(m.kind, METHOD_RETURN | ERROR)) else { continue };
                if to == serial {
                    break m;
                }
                if self.waiting.contains(&to) {
                    self.early.push((to, m));
                }
                continue;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "no reply in time"));
            }
            self.stream.set_read_timeout(Some(left))?;
            let mut chunk = [0u8; 4096];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the bus hung up")),
                Ok(n) => self.inbox.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(timed_out(e, "no reply in time")),
            }
        };
        if message.kind == ERROR {
            let reply = Reply { signature: message.signature, body: message.body, big_endian: message.big_endian };
            // The first argument of an error is its text, when there is one.
            let text = if reply.signature.starts_with('s') { reply.first_string().unwrap_or_default() } else { String::new() };
            return Err(io::Error::other(Refused { name: message.error_name.unwrap_or_default(), message: text }));
        }
        Ok(Reply { signature: message.signature, body: message.body, big_endian: message.big_endian })
    }

    /// Sends a call and waits for its reply.
    pub fn call(&mut self, call: &Call) -> io::Result<Reply> {
        let serial = self.send(call)?;
        self.reply(serial)
    }
}

impl Reply {
    fn first_string(&self) -> io::Result<String> {
        self.reader().string()
    }

    /// The return value of a method that returns an array of strings.
    pub fn strings(&self) -> io::Result<Vec<String>> {
        self.expect("as")?;
        let mut r = self.reader();
        let end = r.array(4)?;
        let mut list = Vec::new();
        while r.pos < end {
            list.push(r.string()?);
        }
        Ok(list)
    }

    /// The return value of a method that returns an array of 32-bit integers.
    /// The return value of a method that returns a boolean.
    pub fn boolean(&self) -> io::Result<bool> {
        self.expect("b")?;
        Ok(self.reader().u32()? != 0)
    }

    pub fn ints(&self) -> io::Result<Vec<i32>> {
        self.expect("ai")?;
        let mut r = self.reader();
        let end = r.array(4)?;
        let mut list = Vec::new();
        while r.pos < end {
            list.push(r.u32()? as i32);
        }
        Ok(list)
    }

    fn expect(&self, signature: &str) -> io::Result<()> {
        if self.signature == signature {
            Ok(())
        } else {
            Err(bad(format!("the reply holds {:?}, not {signature:?}", self.signature)))
        }
    }

    fn reader(&self) -> Reader<'_> {
        Reader { buf: &self.body, pos: 0, big_endian: self.big_endian }
    }
}

/// The session bus's address: `DBUS_SESSION_BUS_ADDRESS`, or else the socket systemd keeps in
/// `XDG_RUNTIME_DIR`.
pub fn session_address() -> Option<String> {
    address_from(std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(), std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from))
}

fn address_from(env: Option<String>, runtime_dir: Option<PathBuf>) -> Option<String> {
    env.filter(|a| !a.is_empty()).or_else(|| Some(format!("unix:path={}", runtime_dir?.join("bus").display())))
}

/// Whether `path` is a valid object path: `/`, or `/`-separated names of ASCII letters,
/// digits and `_`. The bus drops a client that sends any other.
pub fn valid_path(path: &str) -> bool {
    path == "/"
        || path.strip_prefix('/').is_some_and(|rest| {
            rest.split('/').all(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        })
}

#[derive(Debug, PartialEq)]
enum Socket {
    Path(PathBuf),
    Abstract(Vec<u8>),
}

/// The Unix sockets a D-Bus address names, in order. Other transports are skipped.
fn sockets(address: &str) -> Vec<Socket> {
    address
        .split(';')
        .filter_map(|entry| {
            let keys = entry.strip_prefix("unix:")?;
            keys.split(',').find_map(|pair| match pair.split_once('=')? {
                ("path", value) => Some(Socket::Path(PathBuf::from(std::ffi::OsString::from_vec(unescape(value)?)))),
                ("abstract", value) => Some(Socket::Abstract(unescape(value)?)),
                _ => None,
            })
        })
        .collect()
}

/// An address value with its `%XX` escapes undone. None when an escape is broken.
fn unescape(value: &str) -> Option<Vec<u8>> {
    let b = value.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(out)
}

/// The login line's user ID: the decimal number, each digit as two hex digits.
fn uid_hex(uid: u32) -> String {
    uid.to_string().bytes().map(|b| format!("{b:02x}")).collect()
}

/// One `\r\n`-ended line of the login, read a byte at a time so nothing after it is taken.
fn read_line(stream: &mut UnixStream) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while !line.ends_with(b"\r\n") {
        if line.len() > 512 {
            return Err(bad("the bus's login line is too long".to_string()));
        }
        match stream.read(&mut byte) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the bus hung up during the login")),
            Ok(_) => line.push(byte[0]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    line.truncate(line.len() - 2);
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// A read or write that ran out of time says so with `TimedOut`, whatever the socket reported.
fn timed_out(e: io::Error, what: &str) -> io::Error {
    match e.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => io::Error::new(io::ErrorKind::TimedOut, what.to_string()),
        _ => e,
    }
}

fn bad(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// A received message: only what keyglowd looks at.
#[derive(Debug)]
struct Message {
    kind: u8,
    reply_serial: Option<u32>,
    error_name: Option<String>,
    signature: String,
    body: Vec<u8>,
    big_endian: bool,
}

/// A method call, little-endian, ready to send.
fn encode(call: &Call, serial: u32) -> Vec<u8> {
    let mut body = Writer::default();
    let signature = match call.args {
        Args::None => "",
        Args::Bytes(bytes) => {
            body.u32(bytes.len() as u32);
            body.buf.extend_from_slice(bytes);
            "ay"
        }
    };
    let mut m = Writer::default();
    m.buf.extend_from_slice(&[b'l', METHOD_CALL, NO_AUTO_START, 1]);
    m.u32(body.buf.len() as u32);
    m.u32(serial);
    // The header fields' length, filled in once they are written.
    m.u32(0);
    m.field(PATH, b'o');
    m.string(call.path);
    m.field(INTERFACE, b's');
    m.string(call.interface);
    m.field(MEMBER, b's');
    m.string(call.member);
    m.field(DESTINATION, b's');
    m.string(call.destination);
    if !signature.is_empty() {
        m.field(SIGNATURE, b'g');
        m.signature(signature);
    }
    let fields = (m.buf.len() - 16) as u32;
    m.buf[12..16].copy_from_slice(&fields.to_le_bytes());
    // The body starts on a multiple of 8, so its own alignment counts from its start.
    m.pad(8);
    m.buf.extend_from_slice(&body.buf);
    m.buf
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn pad(&mut self, to: usize) {
        self.buf.resize(self.buf.len().next_multiple_of(to), 0);
    }

    fn u32(&mut self, v: u32) {
        self.pad(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn string(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    fn signature(&mut self, s: &str) {
        self.buf.push(s.len() as u8);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    /// A header field's code and the one-letter type of its value.
    fn field(&mut self, code: u8, kind: u8) {
        self.pad(8);
        self.buf.extend_from_slice(&[code, 1, kind, 0]);
    }
}

/// The message at the start of `buf` and its length, or None until all of it has arrived.
fn decode(buf: &[u8]) -> io::Result<Option<(Message, usize)>> {
    if buf.len() < 16 {
        return Ok(None);
    }
    let big_endian = match buf[0] {
        b'l' => false,
        b'B' => true,
        other => return Err(bad(format!("a message in an unknown byte order ({other:#x})"))),
    };
    if buf[3] != 1 {
        return Err(bad(format!("a message in protocol version {}", buf[3])));
    }
    let head = Reader { buf, pos: 0, big_endian };
    let body_len = head.u32_at(4) as usize;
    let fields_end = 16 + head.u32_at(12) as usize;
    let body_start = fields_end.next_multiple_of(8);
    let total = body_start.saturating_add(body_len);
    if total > MAX_MESSAGE {
        return Err(bad(format!("a message of {total} bytes")));
    }
    if buf.len() < total {
        return Ok(None);
    }
    let mut m = Message {
        kind: buf[1],
        reply_serial: None,
        error_name: None,
        signature: String::new(),
        body: buf[body_start..total].to_vec(),
        big_endian,
    };
    let mut r = Reader { buf: &buf[..fields_end], pos: 16, big_endian };
    while r.pos < fields_end {
        r.pad(8)?;
        let code = r.byte()?;
        let kind = r.signature()?;
        match (code, kind.as_str()) {
            (REPLY_SERIAL, "u") => m.reply_serial = Some(r.u32()?),
            (ERROR_NAME, "s") => m.error_name = Some(r.string()?),
            (SIGNATURE, "g") => m.signature = r.signature()?,
            (_, "s" | "o") => drop(r.string()?),
            (_, "g") => drop(r.signature()?),
            (_, "y") => drop(r.byte()?),
            (_, "u" | "i" | "b") => drop(r.u32()?),
            _ => return Err(bad(format!("a header field of type {kind:?}"))),
        }
    }
    Ok(Some((m, total)))
}

/// Reads values from a message or a body, keeping to the protocol's alignment.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    big_endian: bool,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len()).ok_or_else(|| bad("a message ends too soon".to_string()))?;
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn pad(&mut self, to: usize) -> io::Result<()> {
        let n = self.pos.next_multiple_of(to) - self.pos;
        self.take(n).map(drop)
    }

    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32_at(&self, at: usize) -> u32 {
        let b: [u8; 4] = self.buf[at..at + 4].try_into().unwrap();
        if self.big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    }

    fn u32(&mut self) -> io::Result<u32> {
        self.pad(4)?;
        let at = self.pos;
        self.take(4)?;
        Ok(self.u32_at(at))
    }

    fn text(&mut self, len: usize) -> io::Result<String> {
        let bytes = self.take(len)?.to_vec();
        if self.byte()? != 0 {
            return Err(bad("a string without its ending nul byte".to_string()));
        }
        String::from_utf8(bytes).map_err(|_| bad("a string that isn't UTF-8".to_string()))
    }

    fn string(&mut self) -> io::Result<String> {
        let len = self.u32()? as usize;
        self.text(len)
    }

    fn signature(&mut self) -> io::Result<String> {
        let len = self.byte()? as usize;
        self.text(len)
    }

    /// Starts an array of elements aligned to `align` and returns where it ends.
    fn array(&mut self, align: usize) -> io::Result<usize> {
        let len = self.u32()? as usize;
        self.pad(align)?;
        let end = self.pos + len;
        if end > self.buf.len() {
            return Err(bad("an array runs past the end of its message".to_string()));
        }
        Ok(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn a_call_is_laid_out_as_the_spec_says() {
        let call = Call { destination: "d", path: "/p", interface: "i.f", member: "M", args: Args::None };
        #[rustfmt::skip]
        let want: Vec<u8> = vec![
            b'l', 1, NO_AUTO_START, 1, 0, 0, 0, 0, 7, 0, 0, 0, 58, 0, 0, 0,
            // Path "/p"
            1, 1, b'o', 0, 2, 0, 0, 0, b'/', b'p', 0, 0, 0, 0, 0, 0,
            // Interface "i.f"
            2, 1, b's', 0, 3, 0, 0, 0, b'i', b'.', b'f', 0, 0, 0, 0, 0,
            // Member "M"
            3, 1, b's', 0, 1, 0, 0, 0, b'M', 0, 0, 0, 0, 0, 0, 0,
            // Destination "d", then padding to 8
            6, 1, b's', 0, 1, 0, 0, 0, b'd', 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(encode(&call, 7), want);
    }

    #[test]
    fn a_call_with_bytes_carries_their_signature_and_length() {
        let call = Call { destination: "d", path: "/p", interface: "i.f", member: "M", args: Args::Bytes(&[9, 8, 7]) };
        let m = encode(&call, 1);
        // The body: the array's length, then the bytes.
        assert_eq!(&m[4..8], &7u32.to_le_bytes());
        assert_eq!(&m[12..16], &72u32.to_le_bytes());
        assert_eq!(&m[80..88], &[8, 1, b'g', 0, 2, b'a', b'y', 0]);
        assert_eq!(&m[88..], &[3, 0, 0, 0, 9, 8, 7]);
    }

    /// A reply as a peer would send it, in either byte order.
    fn reply_bytes(big: bool, kind: u8, reply_to: u32, error: Option<&str>, signature: &str, body: &[u8]) -> Vec<u8> {
        let u = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
        let mut m = vec![if big { b'B' } else { b'l' }, kind, 1, 1];
        m.extend_from_slice(&u(body.len() as u32));
        m.extend_from_slice(&u(99));
        m.extend_from_slice(&[0; 4]);
        m.extend_from_slice(&[REPLY_SERIAL, 1, b'u', 0]);
        m.extend_from_slice(&u(reply_to));
        if let Some(name) = error {
            m.extend_from_slice(&[ERROR_NAME, 1, b's', 0]);
            m.extend_from_slice(&u(name.len() as u32));
            m.extend_from_slice(name.as_bytes());
            m.push(0);
        }
        if !signature.is_empty() {
            m.resize(m.len().next_multiple_of(8), 0);
            m.extend_from_slice(&[SIGNATURE, 1, b'g', 0, signature.len() as u8]);
            m.extend_from_slice(signature.as_bytes());
            m.push(0);
        }
        let len = u((m.len() - 16) as u32);
        m[12..16].copy_from_slice(&len);
        m.resize(m.len().next_multiple_of(8), 0);
        m.extend_from_slice(body);
        m
    }

    fn ints_body(big: bool, values: &[i32]) -> Vec<u8> {
        let mut b = if big { ((values.len() * 4) as u32).to_be_bytes() } else { ((values.len() * 4) as u32).to_le_bytes() }.to_vec();
        for v in values {
            b.extend_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        }
        b
    }

    fn strings_body(list: &[&str]) -> Vec<u8> {
        let mut b = vec![0; 4];
        for s in list {
            b.resize(b.len().next_multiple_of(4), 0);
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        }
        let len = (b.len() - 4) as u32;
        b[..4].copy_from_slice(&len.to_le_bytes());
        b
    }

    #[test]
    fn replies_decode_in_either_byte_order() {
        for big in [false, true] {
            let bytes = reply_bytes(big, METHOD_RETURN, 5, None, "ai", &ints_body(big, &[0x1532, 0x0A24]));
            let (m, len) = decode(&bytes).unwrap().unwrap();
            assert_eq!(len, bytes.len());
            assert_eq!((m.kind, m.reply_serial, m.error_name.as_deref()), (METHOD_RETURN, Some(5), None));
            let reply = Reply { signature: m.signature, body: m.body, big_endian: m.big_endian };
            assert_eq!(reply.ints().unwrap(), [0x1532, 0x0A24]);
        }
    }

    #[test]
    fn booleans_decode() {
        for (v, want) in [(0u32, false), (1, true)] {
            let bytes = reply_bytes(false, METHOD_RETURN, 1, None, "b", &v.to_le_bytes());
            let (m, _) = decode(&bytes).unwrap().unwrap();
            assert_eq!(Reply { signature: m.signature, body: m.body, big_endian: false }.boolean().unwrap(), want);
        }
    }

    #[test]
    fn string_arrays_decode() {
        let bytes = reply_bytes(false, METHOD_RETURN, 1, None, "as", &strings_body(&["IO2335F55401610", "PM1", ""]));
        let (m, _) = decode(&bytes).unwrap().unwrap();
        let reply = Reply { signature: m.signature, body: m.body, big_endian: false };
        assert_eq!(reply.strings().unwrap(), ["IO2335F55401610", "PM1", ""]);
        // Asking for another type than the reply has is an error, not a guess.
        assert!(reply.ints().is_err());
    }

    #[test]
    fn a_message_is_only_decoded_once_all_of_it_is_there() {
        let mut bytes = reply_bytes(false, METHOD_RETURN, 1, None, "as", &strings_body(&["abc"]));
        let whole = bytes.len();
        for cut in 0..whole {
            assert!(decode(&bytes[..cut]).unwrap().is_none(), "decoded from {cut} of {whole} bytes");
        }
        // The next message's first bytes may follow.
        bytes.extend_from_slice(b"l\x02");
        assert_eq!(decode(&bytes).unwrap().unwrap().1, whole);
    }

    #[test]
    fn broken_messages_are_errors() {
        let good = reply_bytes(false, METHOD_RETURN, 1, None, "", &[]);
        let mut order = good.clone();
        order[0] = b'x';
        assert!(decode(&order).is_err());
        let mut version = good.clone();
        version[3] = 2;
        assert!(decode(&version).is_err());
        let mut huge = good.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&huge).is_err());
        // A header field array that runs past its own end.
        let mut fields = good.clone();
        fields[12..16].copy_from_slice(&3u32.to_le_bytes());
        assert!(decode(&fields).is_err());
        // A body shorter than its array says.
        let short = reply_bytes(false, METHOD_RETURN, 1, None, "ai", &[40, 0, 0, 0, 1, 0, 0, 0]);
        let (m, _) = decode(&short).unwrap().unwrap();
        assert!(Reply { signature: m.signature, body: m.body, big_endian: false }.ints().is_err());
    }

    #[test]
    fn addresses_name_unix_sockets() {
        assert_eq!(sockets("unix:path=/run/user/1000/bus"), [Socket::Path("/run/user/1000/bus".into())]);
        assert_eq!(
            sockets("unix:abstract=/tmp/dbus-AbC,guid=0123456789abcdef0123456789abcdef"),
            [Socket::Abstract(b"/tmp/dbus-AbC".to_vec())]
        );
        assert_eq!(
            sockets("tcp:host=localhost,port=1;unix:guid=1,path=/a%20b%2c;unix:path=/c"),
            [Socket::Path("/a b,".into()), Socket::Path("/c".into())]
        );
        assert_eq!(sockets("unix:tmpdir=/tmp"), []);
        assert_eq!(sockets("unix:path=/bad%zz"), []);
        assert_eq!(sockets("nonsense"), []);
        assert_eq!(sockets(""), []);
    }

    #[test]
    fn the_session_address_falls_back_to_the_runtime_dir() {
        assert_eq!(address_from(Some("unix:path=/x".into()), Some("/run/user/7".into())).as_deref(), Some("unix:path=/x"));
        assert_eq!(address_from(None, Some("/run/user/7".into())).as_deref(), Some("unix:path=/run/user/7/bus"));
        assert_eq!(address_from(Some(String::new()), Some("/run/user/7".into())).as_deref(), Some("unix:path=/run/user/7/bus"));
        assert_eq!(address_from(None, None), None);
    }

    #[test]
    fn object_paths_are_checked() {
        for good in ["/", "/org/razer", "/org/razer/device/IO2335F55401610", "/a_1/B"] {
            assert!(valid_path(good), "{good}");
        }
        for bad in ["", "org", "/org/", "//org", "/org//razer", "/org/razer-1", "/org/ra zer", "/é"] {
            assert!(!valid_path(bad), "{bad}");
        }
    }

    #[test]
    fn the_login_spells_the_user_id_in_hex() {
        assert_eq!(uid_hex(1000), "31303030");
        assert_eq!(uid_hex(0), "30");
    }

    /// A bus that logs the client in, answers Hello, then hands each later call's serial and
    /// member to `script`, which returns the bytes to send back.
    fn fake_bus(script: impl Fn(u32, &str) -> Vec<u8> + Send + 'static) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bus");
        let listener = UnixListener::bind(&socket).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while !buf.windows(2).any(|w| w == b"\r\n") {
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            let end = buf.windows(2).position(|w| w == b"\r\n").unwrap();
            if !buf[..end].starts_with(b"\0AUTH EXTERNAL ") {
                return;
            }
            buf.drain(..end + 2);
            s.write_all(b"OK 0123456789abcdef0123456789abcdef\r\n").unwrap();
            loop {
                if let Some(rest) = buf.strip_prefix(b"BEGIN\r\n") {
                    buf = rest.to_vec();
                    break;
                }
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            loop {
                while buf.len() >= 16 {
                    let total = (16 + u32::from_le_bytes(buf[12..16].try_into().unwrap()) as usize).next_multiple_of(8)
                        + u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
                    if buf.len() < total {
                        break;
                    }
                    let serial = u32::from_le_bytes(buf[8..12].try_into().unwrap());
                    let msg: Vec<u8> = buf.drain(..total).collect();
                    let member = ["Hello", "Fail", "Slow", "Shuffle", "Strings", "Hang up"]
                        .into_iter()
                        .find(|m| msg.windows(m.len()).any(|w| w == m.as_bytes()))
                        .unwrap_or("?");
                    let out = if member == "Hello" {
                        let mut body = 5u32.to_le_bytes().to_vec();
                        body.extend_from_slice(b":1.42\0");
                        reply_bytes(false, METHOD_RETURN, serial, None, "s", &body)
                    } else {
                        script(serial, member)
                    };
                    if member == "Hang up" {
                        return;
                    }
                    if s.write_all(&out).is_err() {
                        return;
                    }
                }
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
        });
        let address = format!("unix:path={}", socket.display());
        (dir, address)
    }

    fn call(member: &str) -> Call<'_> {
        Call { destination: "org.example", path: "/", interface: "org.example", member, args: Args::None }
    }

    #[test]
    fn calls_get_their_replies_past_other_messages() {
        let (_dir, address) = fake_bus(|serial, member| match member {
            "Strings" => {
                // A signal and a reply to some other call come first.
                let mut out = reply_bytes(false, 4, 0, None, "", &[]);
                out.extend(reply_bytes(false, METHOD_RETURN, serial + 100, None, "", &[]));
                out.extend(reply_bytes(false, METHOD_RETURN, serial, None, "as", &strings_body(&["a", "bc"])));
                out
            }
            _ => reply_bytes(false, METHOD_RETURN, serial, None, "", &[]),
        });
        let mut bus = Bus::connect(&address, TIMEOUT).unwrap();
        assert_eq!(bus.call(&call("Strings")).unwrap().strings().unwrap(), ["a", "bc"]);
        assert_eq!(bus.call(&call("Strings")).unwrap().strings().unwrap(), ["a", "bc"]);
    }

    #[test]
    fn replies_that_come_out_of_order_are_kept_for_their_call() {
        // Each "Shuffle" reply is held back and sent after the next one's.
        let held = std::sync::Mutex::new(Vec::new());
        let (_dir, address) = fake_bus(move |serial, _| {
            let mut held = held.lock().unwrap();
            let mut body = 4u32.to_le_bytes().to_vec();
            body.extend_from_slice(&(serial as i32).to_le_bytes());
            let reply = reply_bytes(false, METHOD_RETURN, serial, None, "ai", &body);
            if held.is_empty() {
                *held = reply;
                Vec::new()
            } else {
                let mut out = reply;
                out.append(&mut held);
                out
            }
        });
        let mut bus = Bus::connect(&address, TIMEOUT).unwrap();
        let a = bus.send(&call("Shuffle")).unwrap();
        let b = bus.send(&call("Shuffle")).unwrap();
        assert_eq!(bus.reply(a).unwrap().ints().unwrap(), [a as i32]);
        assert_eq!(bus.reply(b).unwrap().ints().unwrap(), [b as i32]);
    }

    #[test]
    fn an_error_reply_says_what_was_refused() {
        let (_dir, address) = fake_bus(|serial, _| {
            let mut body = 8u32.to_le_bytes().to_vec();
            body.extend_from_slice(b"not here\0");
            reply_bytes(false, ERROR, serial, Some("org.freedesktop.DBus.Error.NameHasNoOwner"), "s", &body)
        });
        let mut bus = Bus::connect(&address, TIMEOUT).unwrap();
        let e = bus.call(&call("Fail")).unwrap_err();
        let r = refused(&e).expect("an error reply");
        assert_eq!((r.name.as_str(), r.message.as_str()), ("org.freedesktop.DBus.Error.NameHasNoOwner", "not here"));
        assert_eq!(e.to_string(), "org.freedesktop.DBus.Error.NameHasNoOwner: not here");
    }

    #[test]
    fn a_missing_reply_times_out() {
        let (_dir, address) = fake_bus(|_, _| Vec::new());
        let mut bus = Bus::connect(&address, Duration::from_millis(200)).unwrap();
        let start = Instant::now();
        let e = bus.call(&call("Slow")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut, "{e}");
        assert!(refused(&e).is_none());
        let took = start.elapsed();
        assert!(took >= Duration::from_millis(200) && took < Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn a_bus_that_hangs_up_is_an_error() {
        let (_dir, address) = fake_bus(|_, _| Vec::new());
        let mut bus = Bus::connect(&address, TIMEOUT).unwrap();
        let e = bus.call(&call("Hang up")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof, "{e}");
    }

    #[test]
    fn connecting_fails_without_a_bus() {
        let dir = tempfile::tempdir().unwrap();
        let address = format!("unix:path={}", dir.path().join("nothing").display());
        assert!(Bus::connect(&address, TIMEOUT).is_err());
        assert!(Bus::connect("tcp:host=localhost,port=1", TIMEOUT).is_err());
    }

    #[test]
    fn a_refused_login_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bus");
        let listener = UnixListener::bind(&socket).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut chunk = [0u8; 256];
            let _ = s.read(&mut chunk);
            let _ = s.write_all(b"REJECTED EXTERNAL\r\n");
        });
        let e = Bus::connect(&format!("unix:path={}", socket.display()), TIMEOUT).err().unwrap();
        assert!(e.to_string().contains("REJECTED"), "{e}");
    }

    /// Talks to the real session bus. Run it by hand with `--ignored` on a desktop.
    #[test]
    #[ignore]
    fn the_real_session_bus_answers() {
        let mut bus = Bus::connect(&session_address().unwrap(), TIMEOUT).unwrap();
        let names = bus
            .call(&Call {
                destination: "org.freedesktop.DBus",
                path: "/org/freedesktop/DBus",
                interface: "org.freedesktop.DBus",
                member: "ListNames",
                args: Args::None,
            })
            .unwrap()
            .strings()
            .unwrap();
        assert!(names.iter().any(|n| n == "org.freedesktop.DBus"), "{names:?}");
    }
}
