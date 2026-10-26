//! The OpenRazer daemon's D-Bus interface: the way OpenRazer means other programs to light its
//! devices. The driver's files under /sys are the daemon's own and can change in any release,
//! so frames only go to them while the daemon isn't running.

use crate::bus::{self, Args, Bus, Call};
use keyglow_core::keyboards::{self, Model};
use std::io;
use std::time::Duration;

/// How long looking for the keyboard may wait for each answer. The daemon answers in about a
/// millisecond; one that holds its name but doesn't answer mustn't hold the lighting up long.
pub const LOOKUP_TIMEOUT: Duration = Duration::from_millis(250);

const NAME: &str = "org.razer";
const ROOT: &str = "/org/razer";
/// Each device's object path is this followed by its serial number.
const DEVICES: &str = "/org/razer/device/";
const CHROMA: &str = "razer.device.lighting.chroma";
const RAZER_VID: i32 = 0x1532;

fn call<'a>(path: &'a str, interface: &'a str, member: &'a str, args: Args<'a>) -> Call<'a> {
    Call { destination: NAME, path, interface, member, args }
}

/// Whether a call failed because the daemon isn't on the bus.
fn not_running(e: &io::Error) -> bool {
    bus::refused(e).is_some_and(|r| {
        matches!(r.name.as_str(), "org.freedesktop.DBus.Error.NameHasNoOwner" | "org.freedesktop.DBus.Error.ServiceUnknown")
    })
}

/// The object path of the keyboard to light and its model, when the daemon is running and
/// serves a keyboard Keyglow knows. With several, the first by path wins.
pub fn find(bus: &mut Bus) -> io::Result<Option<(String, &'static Model)>> {
    let serials = match bus.call(&call(ROOT, "razer.devices", "getDevices", Args::None)) {
        Ok(reply) => reply.strings()?,
        Err(e) if not_running(&e) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut devices = Vec::new();
    for serial in serials {
        let path = format!("{DEVICES}{serial}");
        if !bus::valid_path(&path) {
            continue;
        }
        match bus.call(&call(&path, "razer.device.misc", "getVidPid", Args::None)) {
            Ok(reply) => devices.push((path, reply.ints()?)),
            Err(e) if not_running(&e) => return Ok(None),
            // A device that went away meanwhile, or one without IDs.
            Err(e) if bus::refused(&e).is_some() => {}
            Err(e) => return Err(e),
        }
    }
    Ok(pick(devices))
}

/// Whether the daemon's effect sync is on. It then hands every effect, custom frames too, on
/// to the user's other Razer devices, changing their lighting.
pub fn syncs_effects(bus: &mut Bus) -> io::Result<bool> {
    bus.call(&call(ROOT, "razer.devices", "getSyncEffects", Args::None))?.boolean()
}

/// The first device by object path whose vendor and product IDs are a keyboard Keyglow knows.
fn pick(mut devices: Vec<(String, Vec<i32>)>) -> Option<(String, &'static Model)> {
    devices.sort();
    devices.into_iter().find_map(|(path, ids)| match ids[..] {
        [RAZER_VID, pid] => Some((path, keyboards::by_pid(u16::try_from(pid).ok()?)?)),
        _ => None,
    })
}

/// Shows a frame: the daemon writes the bytes to the driver's frame file, then turns custom
/// mode on to show them, the same two steps as writing the files directly.
pub fn show(bus: &mut Bus, path: &str, frame: &[u8]) -> io::Result<()> {
    // Both calls go out before either reply is awaited, so a frame costs one round trip.
    let rows = bus.send(&call(path, CHROMA, "setKeyRow", Args::Bytes(frame)))?;
    let custom = bus.send(&call(path, CHROMA, "setCustom", Args::None))?;
    bus.reply(rows)?;
    bus.reply(custom)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(path: &str, ids: &[i32]) -> (String, Vec<i32>) {
        (path.to_string(), ids.to_vec())
    }

    #[test]
    fn picks_the_first_keyboard_keyglow_knows() {
        let devices = vec![
            d("/org/razer/device/Z", &[RAZER_VID, 0x0A24]),
            d("/org/razer/device/A", &[RAZER_VID, 0x0084]),
            d("/org/razer/device/M", &[RAZER_VID, 0x024E]),
        ];
        let (path, model) = pick(devices).unwrap();
        assert_eq!((path.as_str(), model.name), ("/org/razer/device/M", "Razer BlackWidow V3"));
    }

    #[test]
    fn skips_devices_that_are_not_keyboards_keyglow_knows() {
        let devices = vec![
            // A mouse, another vendor, IDs out of range, and malformed answers.
            d("/a", &[RAZER_VID, 0x0084]),
            d("/b", &[0x046D, 0x0A24]),
            d("/c", &[RAZER_VID, 0x0A24 + 0x10000]),
            d("/d", &[RAZER_VID, -1]),
            d("/e", &[RAZER_VID]),
            d("/f", &[RAZER_VID, 0x0A24, 0]),
        ];
        assert_eq!(pick(devices), None);
        assert_eq!(pick(Vec::new()), None);
    }

    #[test]
    fn knows_when_the_daemon_is_not_running() {
        let refused = |name: &str| io::Error::other(bus::Refused { name: name.to_string(), message: String::new() });
        assert!(not_running(&refused("org.freedesktop.DBus.Error.NameHasNoOwner")));
        assert!(not_running(&refused("org.freedesktop.DBus.Error.ServiceUnknown")));
        assert!(!not_running(&refused("org.freedesktop.DBus.Error.UnknownObject")));
        assert!(!not_running(&io::Error::new(io::ErrorKind::TimedOut, "slow")));
    }
}
