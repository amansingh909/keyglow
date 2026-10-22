# Keyglow design

Keyglow lights a Razer BlackWidow V3 Tenkeyless on Linux with stacked effects, the way Razer's Chroma Studio does on Windows. A tiny background program draws the lighting; a settings window, opened only when wanted, changes it.

## Why

The owner ran a Chroma Studio preset on Windows: a colour wave, random keys twinkling in random colours, and a ripple spreading from every key pressed. On Linux they had only a cyan and purple wave from OpenRGB. Neither OpenRGB nor OpenRazer can draw all three at once with key presses, and the keyboard's own firmware effects are fixed (its wave is rainbow only, at a speed set by the driver). So one program has to draw every frame itself.

The owner cares most about two things: it must be customizable in any way, and it must be super lightweight.

## Goals

- Recreate the owner's look by default: a cyan to purple wave, random-colour twinkles, and a white ripple on each key press.
- Layers like Chroma Studio. Each layer is one effect covering the whole keyboard. Layers stack bottom to top, and each can be turned on or off, reordered, deleted, given an opacity, and edited. Any number of layers, any mix of effects, including the same effect twice.
- These effects, with Razer's names: Wave, Starlight, Ripple, Reactive, Static, Breathing, Spectrum, Wheel, Fire.
- A settings window with the layer list on the left and the chosen layer's settings on the right: colour pickers (as many colours as the effect takes), sliders for speed, size, density and the like, direction where it applies. The keyboard updates while a slider is dragged.
- Named setups: save the current layers under a name, switch between saved setups, rename and delete them.
- Open Razer Chroma Studio preset files (`.ChromaEffects`) and turn their layers into Keyglow layers, so presets found online work.
- Start by itself at login and light the keyboard with no window open.
- Survive the keyboard being unplugged and plugged back in, and the computer sleeping and waking.

## Non-goals

- The mouse, or any other device. Keyboard only.
- Different layers for different keys. Every layer covers the whole keyboard (the owner never used per-key areas).
- Effects that follow music or the screen (Chroma's Audio Meter and Ambient Awareness). They would have to listen or watch all the time.
- An on-screen preview of the lighting in the window. The keyboard is the preview.
- Other keyboards. The code may keep the device details in one place, but only this model is supported and tested.

## Anti-goals

These would make it a failure even if it works:

- Using noticeable CPU or memory in the background, or keeping the fan up.
- Recording, logging or storing what is typed. The program reads key presses only to know which key position to ripple from, and forgets each one once its effect has faded.
- Needing root, a password, or a kernel module beyond the OpenRazer driver already installed.
- Changing OpenRGB's settings or OpenRazer's settings files. The owner turns OpenRGB off for the keyboard themselves.
- Flicker or fighting with another program over the keyboard.

## Constraints and facts

Hardware and access, all checked on this machine:

- The OpenRazer kernel driver (`razerkbd`) is installed and the OpenRazer daemon runs. The keyboard (USB 1532:0A24) shows up as several HID interfaces under `/sys/bus/hid/drivers/razerkbd/0003:1532:0A24.*`. Only one of them has the lighting files; find it by the presence of `matrix_custom_frame`. The numbers change every time the keyboard is replugged.
- The lighting grid is 6 rows by 18 columns. Lighting files are owned by root, group `plugdev`, mode 0220 (writes) or 0660; the owner is in `plugdev`, so no root is needed.
- A frame is written to `matrix_custom_frame` as rows of: row index, first column, last column, then 3 bytes (R, G, B) per column. All six rows go in one write. Then a write of `1` to `matrix_effect_custom` shows it. Measured: about 12 ms per frame (the driver waits on the USB transfers), and at 30 frames a second about 2% of one core, almost all of it kernel time in the driver.
- Writing `matrix_effect_custom` with every frame is how OpenRazer's own tools draw. Keep doing that; whether the keyboard shows frames without it is unverified.
- Key presses come from `/dev/input/by-id/usb-Razer_Razer_BlackWidow_V3_Tenkeyless-*event-kbd` (there are two; read both). These are group `plugdev` with an ACL for the logged-in user, and the owner is also in `input`. Read them without grabbing them, so typing still works everywhere.
- Linux key codes map to grid positions with OpenRazer's own table (`KEY_MAPPING` and `EVENT_MAPPING` in `openrazer_daemon/keyboard.py`); for example Esc is row 0 column 1, Pause row 0 column 17, Space row 5 column 7, Right arrow row 5 column 17. Column 0 has no keys on this model. This table hasn't been checked against the real keyboard by eye, so keep it in one place where a wrong position is a one-line fix.
- The OpenRazer daemon stays installed and running. With `devices_off_on_screensaver = True` in the owner's `razer.conf`, it sets the keyboard's hardware brightness (`matrix_brightness`) to 0 when the screen saver starts and restores it on unlock, leaving the effect alone. Keyglow never writes `matrix_brightness`. It reads it (the file is readable by `plugdev`) every 2 seconds while it would be drawing, and draws nothing while it reads 0, so a locked screen costs nothing. The daemon can also set a hardware effect itself (another Razer app, or effect sync). Keyglow can't see that happen, so while the setup is still it resends the last frame every 5 seconds; animated setups win back on the next frame.
- The desktop is COSMIC on Wayland, Pop!_OS 24.04. Rust 1.99 is installed for the user (`~/.cargo/bin`). There is no sudo without a password and no GTK development headers.

Weight limits for the background program:

- While animating at the default 30 frames a second: about 3% of one core in total, kernel time included, and under 20 MB of memory.
- When nothing on the keyboard is moving (only still layers, no fading ripples): no frames are drawn and the program sleeps until something changes.
- No busy polling. Waiting is on file descriptors (key presses, config changes) or on the next frame time. The only timers are slow ones: resend the last frame every 5 seconds while still, read the brightness every 2 seconds while drawing, and rescan every 2 seconds while the keyboard is missing.

## How it's put together (decided)

- **Language:** Rust, a workspace of three crates.
  - `keyglow-core`: the settings model, the effects, layer mixing, the key grid and key code table, frame encoding, the Razer preset importer, and reading and writing the settings files. No device or network access.
  - `keyglowd`: the background program. No windowing libraries.
  - `keyglow`: the settings window, built with egui (eframe).
- **Settings:** the active setup lives in `~/.config/keyglow/active.json`, and saved setups in `~/.config/keyglow/setups/<name>.json`. The window writes `active.json` on every change (atomically: write a temporary file, then rename). The background program watches the folder with inotify and reloads on change. That is the only link between the two, so either can run without the other.
- **Drawing:** effects are functions of time and their own state, deterministic given a time and a random seed, so they can be tested without hardware. Layers mix bottom to top with each layer's opacity. Frames are drawn at the setup's frame rate (default 30, range 10 to 60) only while something moves.
- **Starting at login:** a systemd user service for `keyglowd`, with a low CPU priority. The window gets a desktop entry so it shows in the app launcher. An install script puts both programs in `~/.local/bin`.
- **The owner's default setup:** Wave with colours `#03FFFA` and `#55007F`, a moderate speed and left to right; Starlight with random colours; Ripple in white, all at full opacity.

## Effects and their settings

Every layer has: effect, enabled, opacity (0 to 100%), and the settings below. Speeds are in plain units the window can show as a slider.

- **Static:** one colour.
- **Breathing:** one or two colours, or random; the whole keyboard fades in and out. Speed.
- **Spectrum:** the whole keyboard cycles through all hues. Speed.
- **Wave:** a list of two or more colours, or rainbow; speed; direction (left, right, up, down, or an angle); width of one colour band.
- **Wheel:** like Wave but turning around a centre point. Colours or rainbow, speed, clockwise or not.
- **Starlight:** random keys light up and fade. Colours (a list, or random), how many at once (density), how long each lasts.
- **Reactive:** the pressed key lights up and fades. Colour (or random), how long it lasts.
- **Ripple:** a ring spreads out from the pressed key and fades. Colour (or random), speed, how far it travels, ring thickness.
- **Fire:** flickering warm colours rising from the bottom row. Colours (default red, orange, yellow), speed, height.

Layers with no colour on a key leave it see-through: Starlight, Reactive and Ripple only cover the keys they are lighting, so the wave shows underneath.

## The settings window

- Left: the layers, top layer first, each with an on/off switch. Buttons to add a layer (choosing an effect), delete, and move up or down.
- Right: the selected layer's settings. Colour pickers, sliders, and direction choices for that effect.
- Top: the setup's name, a setup menu (switch, save, save as, rename, delete), "Open Razer preset", overall brightness, and frame rate. Edits apply to the keyboard at once but change a saved setup only when Save is pressed. The window marks the setup as unsaved when it differs from the saved file of the same name, and asks before switching away from unsaved changes.
- A status line: whether the background program is running and whether it sees the keyboard. It learns this from a small status file the background program writes when that changes, not by polling it many times a second.

## Razer preset import

A `.ChromaEffects` file is a zip holding one XML file. Its `EffectLayers` list has one `EffectLayer` per layer, with `Effect` (such as `static`, `wave`, `starlight`, `ripple`), `Visible`, `Alpha` (opacity), and a region with `Colors` (`RzColor` entries with Red, Green, Blue, where a missing channel means 0), `Speed`, `Width`, `Duration`, `Density`, `Start` (`onkeypress` for key-driven layers) and `End`. Razer lists layers bottom first in the sample. Regions and per-device cell lists are ignored, since every Keyglow layer covers the whole keyboard; a layer whose region was smaller than the largest layer's in the file is still imported over the whole keyboard, and the window notes it. Only Ripple and Reactive react to key presses in Keyglow: a Static or Breathing layer set to start on a key press becomes a Reactive layer in its colour, and any other effect set to start on a key press is skipped. `End`, `Cycle` and `Pause` are ignored. An effect Keyglow doesn't have is skipped, and the window says which ones were skipped and what was noted. A sample is at `/home/amansingh/Music/Blue Pink StarLight White Ripple.ChromaEffects` (cyan Static, pink Starlight with Speed 5, Density 3 and long Duration, white Ripple with Speed 20 and Width 300 starting on key press). Treat preset files as untrusted: cap sizes, and refuse zips whose XML is very large or nested strangely.

## Left to the builder

- Crate choices beyond Rust, egui and inotify, and the exact file formats, as long as the settings files stay readable JSON.
- How each effect looks in detail, within the descriptions above, and how Razer's speed numbers map onto Keyglow's.
- Physical key positions for round-looking ripples and wheels (the grid is close to a key's real place, but wide keys like Space sit on one column).
- How the background program finds the keyboard again after a replug (a rescan every few seconds while it's missing is fine).

## Calls made while the owner was away

The owner approved the plan in chat and went to bed, so these were decided by the builder from a design review. Each is easy to change later.

- Presets: no per-layer key-press triggers or partial regions in Keyglow's model; import converts or skips them and says so (see "Razer preset import").
- Locked screen: Keyglow follows OpenRazer's screen-saver dimming instead of fighting it, and pauses drawing while the brightness is 0.
- Taking the keyboard back from another program: a frame resend every 5 seconds while still is allowed; it costs about 12 ms of driver time.
- Saved setups change only on Save, with a warning before switching away from unsaved changes.
- Builders never drive the real keyboard or read key presses. The controller runs one short hardware check at the end; the owner checks key positions by eye, and a wrong key is a one-line fix in the key table.
