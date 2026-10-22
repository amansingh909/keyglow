//! The right-hand panel: one layer's settings, drawn with plain egui widgets. Every edit goes
//! straight into the `Layer`; the app notices the change and writes `active.json`.

use crate::model::{self, PaletteChoice, DIRECTIONS};
use eframe::egui::{self, Color32, Slider, Ui};
use keyglow_core::color::{hsv, Rgb};
use keyglow_core::setup::{Effect, EffectKind, Layer, Palette};

/// Colours remembered while the selected layer is set to Random or Rainbow, so switching back
/// to Colours brings them back. The app clears it when another layer is selected.
pub type ColorMemory = Option<Vec<Rgb>>;

pub fn layer_editor(ui: &mut Ui, layer: &mut Layer, memory: &mut ColorMemory) {
    egui::Grid::new("layer-settings").num_columns(2).spacing([18.0, 10.0]).min_col_width(96.0).show(ui, |ui| {
        let kind = layer.effect.kind();
        ui.label("Effect");
        egui::ComboBox::from_id_salt("effect-kind").selected_text(kind.label()).width(160.0).show_ui(ui, |ui| {
            for k in EffectKind::ALL {
                if ui.selectable_label(k == kind, k.label()).clicked() && k != kind {
                    layer.effect = model::change_effect(&layer.effect, k);
                    *memory = None;
                }
            }
        });
        ui.end_row();

        ui.label("Layer");
        ui.checkbox(&mut layer.enabled, "On").on_hover_text("Turn this layer off to hide it without deleting it.");
        ui.end_row();

        ui.label("Opacity");
        ui.add(Slider::new(&mut layer.opacity, 0..=100).suffix("%"));
        ui.end_row();

        effect_rows(ui, &mut layer.effect, memory);
    });
}

fn effect_rows(ui: &mut Ui, effect: &mut Effect, memory: &mut ColorMemory) {
    let kind = effect.kind();
    match effect {
        Effect::Static { color } => {
            ui.label("Colour");
            color_swatch(ui, color, |_| {});
            ui.end_row();
        }
        Effect::Breathing { palette, speed } => {
            palette_rows(ui, kind, palette, memory);
            speed_row(ui, speed);
        }
        Effect::Spectrum { speed } => speed_row(ui, speed),
        Effect::Wave { palette, speed, direction, band } => {
            palette_rows(ui, kind, palette, memory);
            speed_row(ui, speed);
            ui.label("Direction");
            ui.vertical(|ui| {
                ui.add(Slider::new(direction, 0..=359).suffix("°")).on_hover_text(
                    "0° moves left to right, 90° bottom to top, 180° right to left, 270° top to bottom.",
                );
                ui.horizontal(|ui| {
                    for (name, deg) in DIRECTIONS {
                        let arrow = match name {
                            "Left" => "←",
                            "Right" => "→",
                            "Up" => "↑",
                            _ => "↓",
                        };
                        if ui.selectable_label(*direction == deg, format!("{arrow} {name}")).clicked() {
                            *direction = deg;
                        }
                    }
                });
            });
            ui.end_row();
            ui.label("Band width");
            ui.add(Slider::new(band, 2..=36).suffix(" keys")).on_hover_text("How wide one colour band is.");
            ui.end_row();
        }
        Effect::Wheel { palette, speed, clockwise } => {
            palette_rows(ui, kind, palette, memory);
            speed_row(ui, speed);
            ui.label("Turning");
            ui.checkbox(clockwise, "Clockwise").on_hover_text("Off turns the wheel anticlockwise.");
            ui.end_row();
        }
        Effect::Starlight { palette, density, duration_ms } => {
            palette_rows(ui, kind, palette, memory);
            ui.label("Density");
            ui.add(Slider::new(density, 1..=100)).on_hover_text("How many keys twinkle at once.");
            ui.end_row();
            duration_row(ui, duration_ms, "How long each star lasts.");
        }
        Effect::Reactive { palette, duration_ms } => {
            palette_rows(ui, kind, palette, memory);
            duration_row(ui, duration_ms, "How long a pressed key stays lit.");
        }
        Effect::Ripple { palette, speed, reach, thickness } => {
            palette_rows(ui, kind, palette, memory);
            speed_row(ui, speed);
            ui.label("Reach");
            ui.add(Slider::new(reach, 1..=36).suffix(" keys")).on_hover_text("How far a ring travels before it fades.");
            ui.end_row();
            ui.label("Ring thickness");
            ui.add(Slider::new(thickness, 0.5..=6.0).step_by(0.1).fixed_decimals(1).suffix(" keys"));
            ui.end_row();
        }
        Effect::Fire { palette, speed, height } => {
            palette_rows(ui, kind, palette, memory);
            speed_row(ui, speed);
            ui.label("Height");
            ui.add(Slider::new(height, 1..=100).suffix("%")).on_hover_text("How far up the keyboard the flames reach.");
            ui.end_row();
        }
    }
}

fn speed_row(ui: &mut Ui, speed: &mut u8) {
    ui.label("Speed");
    ui.add(Slider::new(speed, 1..=100));
    ui.end_row();
}

fn duration_row(ui: &mut Ui, ms: &mut u32, hint: &str) {
    ui.label("Fade time");
    ui.add(Slider::new(ms, 100..=10_000).logarithmic(true).suffix(" ms")).on_hover_text(hint);
    ui.end_row();
}

/// A Random/Rainbow/Colours choice where the effect allows more than one, then the colour list.
fn palette_rows(ui: &mut Ui, kind: EffectKind, palette: &mut Palette, memory: &mut ColorMemory) {
    let choices = model::palette_choices(kind);
    let current = PaletteChoice::of(palette);
    if choices.len() > 1 {
        ui.label("Palette");
        ui.horizontal(|ui| {
            for &choice in choices {
                if ui.selectable_label(current == choice, choice.label()).clicked() && choice != current {
                    if let Palette::Colors(colors) = palette {
                        *memory = Some(colors.clone());
                    }
                    *palette = model::palette_for_choice(choice, kind, memory.as_deref());
                }
            }
        });
        ui.end_row();
    }
    if let Palette::Colors(colors) = palette {
        ui.label("Colours");
        color_list(ui, kind, colors);
        ui.end_row();
    }
}

/// Swatches with a remove button each, and an add button, within the effect's limits.
/// Right-click a swatch to type a hex code or move it.
fn color_list(ui: &mut Ui, kind: EffectKind, colors: &mut Vec<Rgb>) {
    let (min, max) = model::color_limits(kind);
    let n = colors.len();
    let mut remove = None;
    let mut swap = None;
    ui.horizontal_wrapped(|ui| {
        for (i, color) in colors.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                color_swatch(ui, color, |ui| {
                    if ui.add_enabled(i > 0, egui::Button::new("← Move left")).clicked() {
                        swap = Some((i, i - 1));
                        ui.close();
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("→ Move right")).clicked() {
                        swap = Some((i, i + 1));
                        ui.close();
                    }
                    if ui.add_enabled(n > min, egui::Button::new("Remove")).clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
                if n > min && ui.small_button("×").on_hover_text("Remove this colour").clicked() {
                    remove = Some(i);
                }
            });
            ui.add_space(6.0);
        }
        if n < max && ui.button("+").on_hover_text("Add a colour").clicked() {
            colors.push(model::next_new_color(colors));
        }
    });
    if let Some((a, b)) = swap {
        colors.swap(a, b);
    }
    if let Some(i) = remove {
        colors.remove(i);
    }
}

/// egui's colour picker button. Its right-click menu has a hex code field, then `menu`.
fn color_swatch(ui: &mut Ui, color: &mut Rgb, menu: impl FnOnce(&mut Ui)) {
    let mut srgb = [color.r, color.g, color.b];
    let resp = ui.color_edit_button_srgb(&mut srgb).on_hover_text(format!("{}  (right-click for more)", color.hex()));
    *color = Rgb { r: srgb[0], g: srgb[1], b: srgb[2] };
    resp.context_menu(|ui| {
        ui.horizontal(|ui| {
            ui.label("Hex");
            hex_field(ui, color);
        });
        menu(ui);
    });
}

/// A text field holding `#RRGGBB`; the colour changes as soon as the text is a valid code.
fn hex_field(ui: &mut Ui, color: &mut Rgb) {
    let id = ui.make_persistent_id("hex");
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| color.hex());
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text).id(id).desired_width(72.0).char_limit(7).font(egui::TextStyle::Monospace),
    );
    if resp.changed() {
        let t = text.trim();
        let parsed = if t.starts_with('#') { Rgb::from_hex(t) } else { Rgb::from_hex(&format!("#{t}")) };
        if let Some(c) = parsed {
            *color = c;
        }
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

/// Up to six colours that sum up a layer in the layer list.
pub enum Preview {
    Colors(Vec<Color32>),
    Random,
}

pub fn preview(effect: &Effect) -> Preview {
    let c = |rgb: &Rgb| Color32::from_rgb(rgb.r, rgb.g, rgb.b);
    let rainbow = || (0..6).map(|i| c(&hsv(i as f32 / 6.0, 1.0, 1.0))).collect();
    match effect {
        Effect::Static { color } => Preview::Colors(vec![c(color)]),
        Effect::Spectrum { .. } => Preview::Colors(rainbow()),
        _ => match effect.palette() {
            Some(Palette::Colors(colors)) => Preview::Colors(colors.iter().take(6).map(c).collect()),
            Some(Palette::Rainbow) => Preview::Colors(rainbow()),
            _ => Preview::Random,
        },
    }
}
