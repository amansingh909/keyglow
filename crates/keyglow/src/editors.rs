//! The right-hand panel: one layer's settings, drawn with plain egui widgets. Every edit goes
//! straight into the `Layer`; the app notices the change and writes `active.json`.

use crate::model::{self, PaletteChoice, DIRECTIONS};
use eframe::egui::{self, Color32, RichText, Sense, Slider, Stroke, StrokeKind, Ui};
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
            color_picker(ui, color);
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
        let picked = color_list(ui, kind, colors);
        ui.end_row();
        if let Some(color) = colors.get_mut(picked) {
            ui.label("");
            color_picker(ui, color);
            ui.end_row();
        }
    }
}

/// Which colour of the list the picker below it is changing. One editor shows at a time, so a
/// fixed id is enough; an index past the end of another layer's list is clamped.
const PICKED: &str = "keyglow-picked-colour";
/// The picker's slider positions and the colour they made (see `model::picker_hsv`).
const PICKER_HSV: &str = "keyglow-picker-hsv";

/// Swatches with a remove button each, and an add button, within the effect's limits. Click a
/// swatch to change it in the picker; right-click it to move or remove it. Returns the index
/// of the picked colour.
fn color_list(ui: &mut Ui, kind: EffectKind, colors: &mut Vec<Rgb>) -> usize {
    let (min, max) = model::color_limits(kind);
    let n = colors.len();
    let picked_id = egui::Id::unique(PICKED);
    let mut picked = ui.data(|d| d.get_temp::<usize>(picked_id)).unwrap_or(0).min(n.saturating_sub(1));
    let mut remove = None;
    let mut swap = None;
    ui.horizontal_wrapped(|ui| {
        for (i, color) in colors.iter().enumerate() {
            ui.push_id(i, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let resp = swatch(ui, *color, egui::vec2(34.0, 24.0), i == picked)
                    .on_hover_text(format!("{}. Click to change it, right-click to move it.", color.hex()));
                if resp.clicked() {
                    picked = i;
                }
                resp.context_menu(|ui| {
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
            picked = colors.len() - 1;
        }
    });
    if let Some((a, b)) = swap {
        colors.swap(a, b);
        if picked == a {
            picked = b;
        } else if picked == b {
            picked = a;
        }
    }
    if let Some(i) = remove {
        colors.remove(i);
        if picked > i || picked >= colors.len() {
            picked = picked.saturating_sub(1);
        }
    }
    ui.data_mut(|d| d.insert_temp(picked_id, picked));
    picked
}

/// The colour picker, laid out in the panel: one-click colours, then colour, vividness and
/// brightness sliders that show what each would give, then a hex code to type or paste.
fn color_picker(ui: &mut Ui, color: &mut Rgb) {
    let hsv_id = egui::Id::unique(PICKER_HSV);
    let last = ui.data(|d| d.get_temp::<(Rgb, [f32; 3])>(hsv_id));
    ui.vertical(|ui| egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::Grid::new("colour-picker").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Quick");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                for row in model::quick_colors().collect::<Vec<_>>().chunks(9) {
                    ui.horizontal(|ui| {
                        for &c in row {
                            let name = if c == Rgb::BLACK { "Off (#000000)".to_owned() } else { c.hex() };
                            if swatch(ui, c, egui::vec2(24.0, 24.0), c == *color).on_hover_text(name).clicked() {
                                *color = c;
                            }
                        }
                    });
                }
            });
            ui.end_row();

            let [mut h, mut s, mut v] = model::picker_hsv(*color, last);
            let mut moved = false;
            ui.label("Colour");
            moved |= strip(ui, &mut h, |t| hsv(t, 1.0, 1.0));
            ui.end_row();
            ui.label("Vividness");
            moved |= strip(ui, &mut s, |t| hsv(h, t, v));
            ui.end_row();
            ui.label("Brightness");
            moved |= strip(ui, &mut v, |t| hsv(h, s, t));
            ui.end_row();
            if moved {
                *color = hsv(h, s, v);
            }
            ui.data_mut(|d| d.insert_temp(hsv_id, (*color, [h, s, v])));

            ui.label("Hex code");
            ui.horizontal(|ui| {
                swatch(ui, *color, egui::vec2(34.0, 22.0), false);
                hex_field(ui, color);
                ui.label(RichText::new("type or paste").weak());
            });
            ui.end_row();
        });
    }));
}

/// A filled colour box. `picked` gives it a ring in the text colour.
fn swatch(ui: &mut Ui, color: Rgb, size: egui::Vec2, picked: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let rect = if resp.hovered() { rect.expand(1.0) } else { rect };
        let p = ui.painter();
        p.rect_filled(rect, 4.0, c32(color));
        if picked {
            p.rect_stroke(rect, 4.0, Stroke::new(2.0, ui.visuals().strong_text_color()), StrokeKind::Outside);
        } else {
            p.rect_stroke(rect, 4.0, ui.visuals().widgets.noninteractive.bg_stroke, StrokeKind::Outside);
        }
    }
    resp
}

/// A slider drawn as the colours it gives along its length: `color_at(t)` for `t` in 0..1.
/// Click or drag anywhere on it. Returns whether `value` was moved.
fn strip(ui: &mut Ui, value: &mut f32, color_at: impl Fn(f32) -> Rgb) -> bool {
    const STEPS: usize = 36;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.spacing().slider_width, 20.0), Sense::click_and_drag());
    let mut moved = false;
    if let Some(pos) = resp.interact_pointer_pos() {
        *value = egui::remap_clamp(pos.x, rect.left()..=rect.right(), 0.0..=1.0);
        moved = true;
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        for i in 0..=STEPS {
            let t = i as f32 / STEPS as f32;
            let x = egui::lerp(rect.left()..=rect.right(), t);
            let c = c32(color_at(t));
            mesh.colored_vertex(egui::pos2(x, rect.top()), c);
            mesh.colored_vertex(egui::pos2(x, rect.bottom()), c);
            if i > 0 {
                let k = 2 * i as u32;
                mesh.add_triangle(k - 2, k - 1, k);
                mesh.add_triangle(k - 1, k, k + 1);
            }
        }
        let p = ui.painter();
        p.add(mesh);
        p.rect_stroke(rect, 0.0, ui.visuals().widgets.noninteractive.bg_stroke, StrokeKind::Outside);
        let x = egui::lerp(rect.left()..=rect.right(), *value);
        let knob = egui::Rect::from_center_size(egui::pos2(x, rect.center().y), egui::vec2(8.0, rect.height() + 8.0));
        p.rect_filled(knob, 3.0, c32(color_at(*value)));
        p.rect_stroke(knob, 3.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Inside);
        p.rect_stroke(knob, 3.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Outside);
    }
    moved
}

fn c32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
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
    let c = |rgb: &Rgb| c32(*rgb);
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
