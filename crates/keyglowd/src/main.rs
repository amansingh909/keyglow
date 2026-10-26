//! keyglowd: lights a Razer keyboard from the active Keyglow setup.

mod bus;
mod device;
mod input;
mod openrazer;
mod run;
mod slice;
mod watch;

use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str =
    "usage: keyglowd [--config-dir DIR] [--sysfs-dir DIR] [--input-dir DIR] [--status-file PATH] [--bus-address ADDRESS|none]";

/// `Ok(None)` means help was asked for.
fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<run::Options>, String> {
    let mut opts = run::Options {
        config_dir: keyglow_core::store::Store::default_dir(),
        sysfs_dir: PathBuf::from(device::DEFAULT_SYSFS_DIR),
        input_dir: PathBuf::from(input::DEFAULT_INPUT_DIR),
        status_file: keyglow_core::store::status_path(),
        bus_address: bus::session_address(),
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--bus-address" {
            let address = args.next().ok_or_else(|| format!("{arg:?} needs a value"))?;
            let address = address.into_string().map_err(|a| format!("bad bus address {a:?}"))?;
            // `none` keeps to the driver's files, for testing.
            opts.bus_address = (address != "none").then_some(address);
            continue;
        }
        let slot = match arg.to_str() {
            Some("--config-dir") => &mut opts.config_dir,
            Some("--sysfs-dir") => &mut opts.sysfs_dir,
            Some("--input-dir") => &mut opts.input_dir,
            Some("--status-file") => &mut opts.status_file,
            Some("-h" | "--help") => return Ok(None),
            _ => return Err(format!("unknown argument {arg:?}")),
        };
        *slot = PathBuf::from(args.next().ok_or_else(|| format!("{arg:?} needs a value"))?);
    }
    Ok(Some(opts))
}

fn main() {
    let opts = match parse_args(std::env::args_os().skip(1)) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("{USAGE}");
            return;
        }
        Err(e) => {
            eprintln!("keyglowd: {e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run::run(&opts) {
        eprintln!("keyglowd: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn options_default_to_the_real_places() {
        let o = parse_args(args(&[])).unwrap().unwrap();
        assert_eq!(o.config_dir, keyglow_core::store::Store::default_dir());
        assert_eq!(o.sysfs_dir, PathBuf::from("/sys/bus/hid/drivers/razerkbd"));
        assert_eq!(o.input_dir, PathBuf::from("/dev/input/by-id"));
        assert_eq!(o.status_file, keyglow_core::store::status_path());
        assert_eq!(o.bus_address, bus::session_address());
    }

    #[test]
    fn options_can_point_elsewhere() {
        let o = parse_args(args(&[
            "--config-dir", "/t/c", "--sysfs-dir", "/t/s", "--input-dir", "/t/i", "--status-file", "/t/st.json",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(o.config_dir, PathBuf::from("/t/c"));
        assert_eq!(o.sysfs_dir, PathBuf::from("/t/s"));
        assert_eq!(o.input_dir, PathBuf::from("/t/i"));
        assert_eq!(o.status_file, PathBuf::from("/t/st.json"));
        let o = parse_args(args(&["--bus-address", "unix:path=/t/bus"])).unwrap().unwrap();
        assert_eq!(o.bus_address.as_deref(), Some("unix:path=/t/bus"));
        let o = parse_args(args(&["--bus-address", "none"])).unwrap().unwrap();
        assert_eq!(o.bus_address, None);
    }

    #[test]
    fn bad_options_are_refused() {
        assert!(parse_args(args(&["--config-dir"])).is_err());
        assert!(parse_args(args(&["--bus-address"])).is_err());
        assert!(parse_args(args(&["--colour", "red"])).is_err());
        assert!(parse_args(args(&["stray"])).is_err());
        assert!(parse_args(args(&["--help"])).unwrap().is_none());
    }
}
