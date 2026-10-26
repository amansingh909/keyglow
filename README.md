<p align="center">
  <img src="docs/media/hero.webp" alt="Keyglow lighting a BlackWidow V3 TKL: a cyan to purple wave, random twinkles and red ripples from typed keys" width="100%">
</p>

<p align="center">
  <a href="../../releases/latest"><img src="https://img.shields.io/github/v/release/amansingh909/keyglow?style=for-the-badge&labelColor=111111&color=44D62C&label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/platform-Linux-44D62C?style=for-the-badge&labelColor=111111&logo=linux&logoColor=white" alt="Linux">
  <img src="https://img.shields.io/badge/built%20with-Rust-44D62C?style=for-the-badge&labelColor=111111&logo=rust&logoColor=white" alt="Built with Rust">
  <a href="#supported-keyboards"><img src="https://img.shields.io/badge/supports-92%20Razer%20models-44D62C?style=for-the-badge&labelColor=111111&logo=razer&logoColor=44D62C" alt="Supports 92 Razer models"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-44D62C?style=for-the-badge&labelColor=111111" alt="MIT license"></a>
</p>

<p align="center">
  <a href="https://github.com/sponsors/amansingh909"><img src="https://img.shields.io/badge/sponsor-on%20GitHub-44D62C?style=for-the-badge&labelColor=111111&logo=githubsponsors&logoColor=EA4AAA" alt="Sponsor on GitHub"></a>
</p>

# Keyglow

Layered keyboard lighting for Razer keyboards on Linux. It works the way Razer Chroma Studio does on Windows: you stack effects, such as a colour wave, twinkling keys and a ripple from every key you press, and they all play at once.

Two programs make it up:

- `keyglowd` is a tiny background program that draws the lighting. It starts at login and needs no window.
- `keyglow` is the settings window. Open it when you want to change something, then close it. The lighting keeps running.

It runs on every Razer keyboard, laptop and keypad that the OpenRazer driver can light key by key, 92 models in all. It was built and tested on a BlackWidow V3 Tenkeyless with Pop!_OS 24.04 and COSMIC on Wayland. The other models should work but haven't been tested yet, so if you try one, please [open an issue](../../issues) and say how it went.

## Effects

<p align="center">
  <img src="docs/media/effects.webp" alt="All nine effects running on a keyboard each: Static, Breathing, Spectrum, Wave, Wheel, Starlight, Reactive, Ripple and Fire" width="100%">
</p>

Every keyboard above is drawn from real Keyglow frames, made by the same code that lights the keyboard. Reactive and Ripple are answering simulated typing.

## Supported keyboards

Keyglow finds the keyboard by its USB ID and sizes every frame to that model's light grid, using the same grid sizes as OpenRazer. Only the BlackWidow V3 Tenkeyless has been tested so far. Everything else on the list should work but hasn't been tried.

Ripple and Reactive need to know where each key sits. Keyglow uses Razer's standard key grid for that, the same one OpenRazer's own ripple uses, so full-size and tenkeyless keyboards should be spot on. On compact keyboards, laptops and keypads some keys may light the wrong spot or nothing at all, while the effects that don't follow keys work the same everywhere. Keyboards lit in zones instead of key by key, such as the Ornata V3, show the effects across their zones.

<details>
<summary>All 92 models</summary>

**Keyboards** (53)

- BlackWidow 2019
- BlackWidow Chroma
- BlackWidow Chroma (Overwatch)
- BlackWidow Chroma Tournament Edition
- BlackWidow Chroma V2
- BlackWidow Elite
- BlackWidow Essential
- BlackWidow Ultimate 2016
- BlackWidow V3
- BlackWidow V3 Mini HyperSpeed (Wired)
- BlackWidow V3 Mini HyperSpeed (Wireless)
- BlackWidow V3 Pro (Wired)
- BlackWidow V3 Pro (Wireless)
- BlackWidow V3 Tenkeyless (tested)
- BlackWidow V4
- BlackWidow V4 75%
- BlackWidow V4 Mini Hyperspeed (Wired)
- BlackWidow V4 Mini Hyperspeed (Wireless)
- BlackWidow V4 Pro
- BlackWidow V4 Tenkeyless HyperSpeed (Wired)
- BlackWidow V4 Tenkeyless HyperSpeed (Wireless)
- BlackWidow V4 X
- BlackWidow X Chroma
- BlackWidow X Chroma Tournament Edition
- BlackWidow X Ultimate
- Cynosa Chroma
- Cynosa Chroma Pro
- Cynosa V2
- DeathStalker Chroma
- DeathStalker V2
- DeathStalker V2 Pro (Wired)
- DeathStalker V2 Pro (Wireless)
- DeathStalker V2 Pro TKL (Wired)
- DeathStalker V2 Pro TKL (Wireless)
- Huntsman
- Huntsman Elite
- Huntsman Mini
- Huntsman Mini (JP)
- Huntsman Mini Analog
- Huntsman Tournament Edition
- Huntsman V2
- Huntsman V2 Analog
- Huntsman V2 Tenkeyless
- Huntsman V3 Pro
- Huntsman V3 Pro 8KHz
- Huntsman V3 Pro Mini
- Huntsman V3 Pro TKL
- Ornata
- Ornata Chroma
- Ornata V2
- Ornata V3
- Ornata V3 Tenkeyless
- Ornata V3 X

**Laptops** (36)

- Blade (Late 2016)
- Blade 14 (2021)
- Blade 14 (2022)
- Blade 14 (2023)
- Blade 14 (2024)
- Blade 14 (2025)
- Blade 15 (2018)
- Blade 15 (2018) Mercury
- Blade 15 (2019) Advanced
- Blade 15 (2023)
- Blade 15 (Mid 2019) Mercury White
- Blade 15 Advanced (2020)
- Blade 15 Advanced (Early 2021)
- Blade 15 Advanced (Early 2022)
- Blade 15 Advanced (Mid 2021)
- Blade 15 Studio Edition (2019)
- Blade 16 (2023)
- Blade 16 (2025)
- Blade 17 (2022)
- Blade 17 Pro (Early 2021)
- Blade 17 Pro (Mid 2021)
- Blade 18 (2023)
- Blade 18 (2024)
- Blade 18 (2025)
- Blade Advanced (Late 2019)
- Blade Pro (2017)
- Blade Pro (2019)
- Blade Pro (Early 2020)
- Blade Pro (Late 2016)
- Blade Pro (Late 2019)
- Blade Pro FullHD (2017)
- Blade Stealth
- Blade Stealth (Late 2016)
- Blade Stealth (Late 2017)
- Blade Stealth (Mid 2017)
- Blade Stealth (QHD)

**Keypads** (3)

- Orbweaver Chroma
- Tartarus Pro
- Tartarus V2

</details>

## What you need

- The OpenRazer kernel driver (`razerkbd`) installed. The OpenRazer daemon can stay running. When it dims the keyboard for the screen saver, `keyglowd` pauses until the brightness comes back.
- Your user in the `plugdev` group. No root is needed to run or install Keyglow.
- Read access to the keyboard's input device, so `keyglowd` can see key presses for Ripple and Reactive. The logged-in user normally has it through the desktop's access list for the device, or through the `input` or `plugdev` group. Without it the lighting still runs, but nothing reacts to keys.
- Linux 6.12 or newer, so the lighting keeps up while the computer is busy. Older kernels work too, but under load the frames and key presses can lag.
- Rust, only if you build it yourself (`~/.cargo/bin/cargo` or `cargo` on your PATH).
- OpenRGB, or anything else that lights the keyboard, turned off for this keyboard. Two programs drawing at once will flicker against each other.

## Install

### From a release

Download `keyglow-v0.2.1-x86_64-linux.tar.gz` from the [releases page](../../releases), then:

    tar xf keyglow-v0.2.1-x86_64-linux.tar.gz
    cd keyglow-v0.2.1-x86_64-linux
    ./install.sh --enable

The programs in the download are already built, so you don't need Rust. They were built on Pop!_OS 24.04 and need glibc 2.39 or newer (Ubuntu 24.04 and later have it). On an older system, build from source instead.

### From source

From the project folder:

    ./install.sh --enable

This first builds a release version at low priority with at most 4 build jobs (set `CARGO_BUILD_JOBS` to change that).

Either way, `install.sh` copies `keyglowd` and `keyglow` to `~/.local/bin`, installs a login service and a launcher entry with its icon, and starts the lighting. Leave out `--enable` to install without starting anything. Running it again later updates an existing install.

## Using the window

Open Keyglow from the app launcher, or run `keyglow`.

<p align="center">
  <img src="docs/media/window.png" alt="The Keyglow settings window with the Wave layer open and its colour picker showing" width="820">
</p>

- The left side lists your layers, top layer first. Each has an on/off switch. Use the buttons to add a layer, delete one, or move it up or down.
- The right side shows the chosen layer's settings: its colours, sliders for speed, size and density, and direction where it applies. Click a colour to change it in the picker underneath. Pick one of the quick colours, drag the colour, vividness and brightness strips, or type or paste a hex code. The keyboard changes while you drag, and effects that are already running carry on rather than starting over.
- The top bar has the setup name, a setup menu (switch, save, save as, rename, delete), Open Razer preset, overall brightness and frame rate.
- The status line shows whether `keyglowd` is running and whether it can see the keyboard.

Effects: Static, Breathing, Spectrum, Wave, Wheel, Starlight, Reactive, Ripple, Fire. You can use any number of layers, and the same effect more than once. Every layer covers the whole keyboard. Starlight, Reactive and Ripple only colour the keys they are lighting, so the layers below show through.

The default setup is a cyan to purple wave, random-colour twinkles and a white ripple on every key press.

### Saved setups

Changes reach the keyboard at once, but a saved setup only changes when you press Save. The window marks a setup as unsaved when it differs from the saved file, and asks before you switch away from unsaved changes.

### Razer presets

Choose Open Razer preset and pick a `.ChromaEffects` file. Its layers become Keyglow layers. Some things cannot carry over, and the window tells you what happened:

- Regions and per-device key lists are ignored. Every layer covers the whole keyboard.
- Only Ripple and Reactive respond to key presses. A Static or Breathing layer that starts on a key press becomes a Reactive layer in the same colour. Any other effect set to start on a key press is skipped.
- Effects Keyglow does not have are skipped.

## How it starts

The install puts a systemd user service at `~/.config/systemd/user/keyglowd.service`. It starts `keyglowd` at login, restarts it if it fails (a crash or an error exit, not when you stop it yourself), and runs it at low priority with a CPU and memory limit. Handy commands:

    systemctl --user status keyglowd
    systemctl --user restart keyglowd
    journalctl --user -u keyglowd

`keyglowd` copes with the keyboard being unplugged and plugged back in, and with sleep and wake. While the keyboard is missing it looks for it again every couple of seconds.

## Cost

While something is animating at 30 frames a second, `keyglowd` uses about 1 to 3% of one core, almost all of it the keyboard driver's USB transfers, and under 3 MB of memory. When nothing is moving it draws nothing and sleeps, except that it resends the last frame every 5 seconds, so another program can't take the keyboard over for long. The window uses nothing when closed and almost nothing when idle.

It also asks the kernel for a short time slice, so it gets a CPU soon after it wakes even while a game or a build keeps every core busy. That changes when it runs, not how much CPU it takes.

## Files

- `~/.config/keyglow/active.json`: the setup on the keyboard now. The window writes it and `keyglowd` watches it.
- `~/.config/keyglow/setups/<name>.json`: your saved setups. They are plain JSON.
- `$XDG_RUNTIME_DIR/keyglow/status.json`: a small status file `keyglowd` writes so the window can show its state.
- `~/.local/bin/keyglowd`, `~/.local/bin/keyglow`, `~/.config/systemd/user/keyglowd.service`, `~/.local/share/applications/keyglow.desktop`, `~/.local/share/icons/hicolor/scalable/apps/keyglow.svg`: what the installer puts down.

For testing, `keyglowd` accepts `--config-dir`, `--sysfs-dir`, `--input-dir` and `--status-file`. The defaults are the real paths.

## Troubleshooting

- The window says the keyboard is not found: check that your model is in the [list](#supported-keyboards), that the OpenRazer driver is loaded and that your user is in `plugdev` (`id`). Log out and in again after joining the group.
- The lighting flickers: something else is lighting the keyboard. Stop OpenRGB or other lighting tools for it.
- The keyboard goes dark when the screen locks: that is OpenRazer dimming it for the screen saver. It comes back on unlock.
- A ripple starts from the wrong key: key positions come from Razer's standard key grid. On a full-size or tenkeyless keyboard that's a bug, and the fix is one line in `crates/keyglow-core/src/keymap.rs`. Compact keyboards and laptops can differ from the grid, so please open an issue with your model.
- Anything else: `journalctl --user -u keyglowd`.

## Privacy

`keyglowd` reads key presses from the keyboard's own input device only to learn which key to ripple from. It never logs, prints or stores them, forgets each press once its effect has faded, and does not grab the keyboard, so typing works everywhere as usual.

## Uninstall

    ./install.sh --uninstall

This stops and disables the service and removes the files the installer put down. Your settings in `~/.config/keyglow` are kept. Delete that folder yourself if you want them gone.

## License

MIT. See [LICENSE](LICENSE).
