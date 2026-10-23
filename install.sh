#!/usr/bin/env bash
# Install Keyglow for the current user, building it first unless it came prebuilt. No root needed.
#   ./install.sh              install (or update an existing install)
#   ./install.sh --enable     also start the lighting now and at every login
#   ./install.sh --uninstall  stop the service and remove the installed files
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin_dir="$HOME/.local/bin"
unit_dir="$HOME/.config/systemd/user"
app_dir="$HOME/.local/share/applications"
unit="$unit_dir/keyglowd.service"
desktop="$app_dir/keyglow.desktop"

mode="install"
enable=0
for arg in "$@"; do
    case "$arg" in
        --enable) enable=1 ;;
        --uninstall) mode="uninstall" ;;
        -h|--help)
            sed -n '2,5p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown option: $arg (try --help)" >&2
            exit 2
            ;;
    esac
done

if [ "$mode" = "uninstall" ]; then
    echo "Stopping and disabling the keyglowd service."
    systemctl --user disable --now keyglowd.service 2>/dev/null || true
    for f in "$bin_dir/keyglowd" "$bin_dir/keyglow" "$unit" "$desktop"; do
        if [ -e "$f" ]; then
            rm -f "$f"
            echo "Removed $f"
        fi
    done
    systemctl --user daemon-reload || true
    echo "Done. Your settings in $HOME/.config/keyglow were left alone."
    exit 0
fi

if [ -x "$here/bin/keyglowd" ] && [ -x "$here/bin/keyglow" ]; then
    # A release download: the programs come already built.
    built="$here/bin"
    echo "Using the prebuilt programs in $built"
else
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        cargo="$HOME/.cargo/bin/cargo"
    elif command -v cargo >/dev/null 2>&1; then
        cargo="$(command -v cargo)"
    else
        echo "Cannot find cargo. Install Rust from https://rustup.rs first." >&2
        exit 1
    fi

    echo "Building Keyglow (this takes a few minutes the first time)."
    # A few jobs at low priority: building on every core has cut the power on a laptop on battery.
    (cd "$here" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" nice -n 10 "$cargo" build --release --workspace)
    built="$here/target/release"
fi

mkdir -p "$bin_dir" "$unit_dir" "$app_dir"

# Stop a running copy first so the binary can be replaced.
restart=0
if systemctl --user is-active --quiet keyglowd.service 2>/dev/null; then
    systemctl --user stop keyglowd.service
    restart=1
fi

install -m 755 "$built/keyglowd" "$bin_dir/keyglowd"
install -m 755 "$built/keyglow" "$bin_dir/keyglow"
echo "Copied keyglowd and keyglow to $bin_dir"

install -m 644 "$here/packaging/keyglowd.service" "$unit"
echo "Installed the login service at $unit"

sed "s|@KEYGLOW_BIN@|$bin_dir/keyglow|" "$here/packaging/keyglow.desktop" > "$desktop"
chmod 644 "$desktop"
echo "Installed the launcher entry at $desktop"

systemctl --user daemon-reload
echo "Reloaded the systemd user services."

if [ "$enable" -eq 1 ]; then
    systemctl --user enable --now keyglowd.service
    echo "Enabled keyglowd: it is running now and will start at every login."
elif [ "$restart" -eq 1 ]; then
    systemctl --user start keyglowd.service
    echo "Restarted keyglowd with the new version."
else
    echo "Not started. Run ./install.sh --enable to light the keyboard now and at login."
fi
