//! The settings window: setup bar on top, layers on the left, the selected layer's settings on
//! the right, the background program's status at the bottom.
//!
//! The window only edits files. Every change is written to `active.json` (throttled); the saved
//! setups change only through the setup menu.

use crate::editors::{self, ColorMemory, Preview};
use crate::model::{self, DaemonState, SaveThrottle};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Id, Key, KeyboardShortcut, Layout, Modifiers, RichText, Sense,
    Slider,
};
use keyglow_core::preset::{self, Imported};
use keyglow_core::setup::{EffectKind, Setup};
use keyglow_core::store::{self, Store};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// How often the status file is read again.
const STATUS_EVERY: Duration = Duration::from_secs(1);
/// Presets bigger than this are not read at all (the importer refuses far smaller ones anyway).
const MAX_PRESET_FILE: u64 = 8 * 1024 * 1024;

const GREEN: Color32 = Color32::from_rgb(0x3C, 0xB3, 0x71);
const AMBER: Color32 = Color32::from_rgb(0xE0, 0xA0, 0x30);
const RED: Color32 = Color32::from_rgb(0xD9, 0x4F, 0x4F);

enum Notice {
    Info(String),
    Error(String),
}

/// Something that replaces the active setup. Asked about first when there are unsaved edits.
enum Pending {
    Load(String),
    Replace(Box<Setup>, Option<String>),
    OwnerDefault,
}

enum Dialog {
    SaveAs(String),
    Rename(String),
    ConfirmDelete,
    Discard(Pending),
}

/// What a modal dialog wants after this frame.
enum Outcome {
    Open,
    Close,
}

type ImportResult = Option<Result<Imported, String>>;

pub struct App {
    store: Store,
    setup: Setup,
    /// The saved setup with the active setup's name, if there is one.
    saved: Option<Setup>,
    saved_names: Vec<String>,
    selected: Option<usize>,
    color_memory: ColorMemory,
    dirty: bool,
    throttle: SaveThrottle,
    notice: Option<Notice>,
    dialog: Option<Dialog>,
    import: Option<Receiver<ImportResult>>,
    status_path: PathBuf,
    daemon: DaemonState,
    daemon_detail: String,
    status_read_at: Option<Instant>,
}

/// Fonts and spacing. The arrows and dots used in labels live in the bundled monospace font, so
/// it becomes a fallback for normal text too (no extra font is loaded).
pub fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.families.entry(FontFamily::Proportional).or_default().push("Hack".to_owned());
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style.spacing.slider_width = 240.0;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}

impl App {
    /// Loads the active setup. If `active.json` cannot be read, says so and starts from the
    /// owner default without writing anything until the user changes something.
    pub fn open(store: Store, status_path: PathBuf) -> App {
        let (setup, notice) = match store.load_active() {
            Ok(setup) => (setup, None),
            Err(e) => (
                Setup::owner_default(),
                Some(Notice::Error(format!(
                    "Could not read {}: {e}. Showing the default setup instead; nothing is written until you change something.",
                    store.active_path().display()
                ))),
            ),
        };
        let mut app = App {
            saved: store.load_setup(&setup.name).ok(),
            selected: model::display_order(&setup).first().copied(),
            store,
            setup,
            saved_names: Vec::new(),
            color_memory: None,
            dirty: false,
            throttle: SaveThrottle::default(),
            notice,
            dialog: None,
            import: None,
            status_path,
            daemon: DaemonState::NotRunning,
            daemon_detail: String::new(),
            status_read_at: None,
        };
        app.refresh_names();
        app
    }

    fn is_unsaved(&self) -> bool {
        model::is_unsaved(&self.setup, self.saved.as_ref())
    }

    fn refresh_names(&mut self) {
        match self.store.list_setups() {
            Ok(names) => self.saved_names = names,
            Err(e) => self.error(format!("Could not list the saved setups: {e}")),
        }
    }

    fn error(&mut self, text: String) {
        self.notice = Some(Notice::Error(text));
    }

    fn select(&mut self, index: Option<usize>) {
        if index != self.selected {
            self.color_memory = None;
        }
        self.selected = index;
    }

    // ---- Writing files --------------------------------------------------------------------

    /// Writes `active.json` when the throttle allows. Returns how long to wait before asking
    /// again when a change is still waiting to be written.
    fn autosave(&mut self, now: Instant) -> Option<Duration> {
        if self.throttle.should_save(now, self.dirty) {
            self.dirty = false;
            if let Err(e) = self.store.save_active(&self.setup) {
                self.error(format!(
                    "Could not write the active setup, so the keyboard may not show the latest change: {e}"
                ));
            }
            None
        } else if self.dirty {
            Some(self.throttle.wait(now))
        } else {
            None
        }
    }

    /// Writes any change still waiting (when the window closes).
    fn flush(&mut self) {
        if self.dirty {
            self.dirty = false;
            if let Err(e) = self.store.save_active(&self.setup) {
                eprintln!("keyglow: could not write the active setup: {e}");
            }
        }
    }

    fn save(&mut self) -> bool {
        match self.store.save_setup(&self.setup) {
            Ok(()) => {
                self.saved = Some(self.setup.clone());
                self.refresh_names();
                true
            }
            Err(e) => {
                self.error(format!("Could not save “{}”: {e}", self.setup.name));
                false
            }
        }
    }

    fn save_as(&mut self, name: String) -> bool {
        let old = std::mem::replace(&mut self.setup.name, name);
        if self.save() {
            self.dirty = true;
            true
        } else {
            self.setup.name = old;
            false
        }
    }

    /// Renames the active setup, and its saved file when there is one.
    fn rename(&mut self, new: String) -> bool {
        let old = self.setup.name.clone();
        if new == old {
            return true;
        }
        if let Err(why) = model::check_setup_name(&new) {
            self.error(format!("Could not rename “{old}”: {why}"));
            return false;
        }
        self.refresh_names();
        if self.saved_names.contains(&new) {
            self.error(format!("A saved setup named “{new}” already exists."));
            return false;
        }
        if self.saved_names.contains(&old) {
            if let Err(e) = self.store.rename_setup(&old, &new) {
                self.error(format!("Could not rename “{old}”: {e}"));
                return false;
            }
        }
        self.setup.name = new;
        self.saved = self.store.load_setup(&self.setup.name).ok();
        self.dirty = true;
        self.refresh_names();
        true
    }

    fn delete_saved(&mut self) {
        let name = self.setup.name.clone();
        match self.store.delete_setup(&name) {
            Ok(()) => {
                self.saved = None;
                self.notice = Some(Notice::Info(format!(
                    "Deleted the saved setup “{name}”. The keyboard keeps showing it until you switch setups."
                )));
            }
            Err(e) => self.error(format!("Could not delete “{name}”: {e}")),
        }
        self.refresh_names();
    }

    // ---- Switching setups -----------------------------------------------------------------

    /// Replaces the active setup, asking first when that would lose edits.
    fn request(&mut self, pending: Pending) {
        if model::ask_before_switching(&self.setup, self.saved.as_ref()) {
            self.dialog = Some(Dialog::Discard(pending));
        } else {
            self.apply(pending);
        }
    }

    fn apply(&mut self, pending: Pending) {
        match pending {
            Pending::Load(name) => match self.store.load_setup(&name) {
                Ok(mut setup) => {
                    // A hand-edited file may hold another name; the file name is the one shown.
                    setup.name = name;
                    self.replace(setup)
                }
                Err(e) => self.error(format!("Could not load “{name}”: {e}")),
            },
            Pending::Replace(setup, message) => {
                self.replace(*setup);
                if let Some(text) = message {
                    self.notice = Some(Notice::Info(text));
                }
            }
            Pending::OwnerDefault => self.replace(Setup::owner_default()),
        }
    }

    fn replace(&mut self, setup: Setup) {
        self.saved = self.store.load_setup(&setup.name).ok();
        self.setup = setup;
        self.color_memory = None;
        self.selected = model::display_order(&self.setup).first().copied();
        self.dirty = true;
    }

    // ---- Razer presets --------------------------------------------------------------------

    /// Opens the file dialog on its own thread so the window keeps drawing meanwhile.
    fn open_preset(&mut self, ctx: &egui::Context) {
        if self.import.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Open a Razer Chroma Studio preset")
                .add_filter("Razer Chroma Studio preset", &["ChromaEffects"])
                .pick_file();
            let _ = tx.send(picked.map(|path| read_preset(&path)));
            ctx.request_repaint();
        });
        self.import = Some(rx);
    }

    fn poll_import(&mut self) {
        let Some(rx) = &self.import else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.import = None;
                match result {
                    None => {}
                    Some(Ok(imported)) => self.imported(imported),
                    Some(Err(e)) => self.error(e),
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.import = None,
        }
    }

    fn imported(&mut self, imported: Imported) {
        let Imported { setup, skipped, notes } = imported;
        let layers = setup.layers.len();
        let mut text = format!("Imported “{}” with {layers} layer{}.", setup.name, if layers == 1 { "" } else { "s" });
        if !skipped.is_empty() {
            text.push_str(&format!(" Skipped, as Keyglow has no such effect: {}.", skipped.join(", ")));
        }
        for note in notes {
            text.push('\n');
            text.push_str(&note);
        }
        self.request(Pending::Replace(Box::new(setup), Some(text)));
    }

    // ---- Status ---------------------------------------------------------------------------

    fn refresh_status(&mut self, now: Instant) {
        // A little early, so the once-a-second repaint always finds it due.
        if self
            .status_read_at
            .is_some_and(|t| now.saturating_duration_since(t) < STATUS_EVERY - Duration::from_millis(100))
        {
            return;
        }
        self.status_read_at = Some(now);
        let status = store::read_status(&self.status_path);
        self.daemon_detail = status.as_ref().map(|s| s.detail.trim().to_owned()).unwrap_or_default();
        self.daemon = model::daemon_state(status, model::keyglowd_alive);
    }

    // ---- Drawing --------------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let unsaved = self.is_unsaved();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            self.setup_menu(ui, unsaved);
            let name = &self.setup.name;
            let shown = if name.chars().count() > 36 { format!("{}…", name.chars().take(35).collect::<String>()) } else { name.clone() };
            ui.label(RichText::new(shown).size(18.0).strong()).on_hover_text(name);
            if unsaved {
                ui.label(RichText::new("● unsaved").color(AMBER)).on_hover_text(
                    "The keyboard shows these changes, but the saved setup does not have them. Press Save to keep them.",
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(unsaved, egui::Button::new("Save")).on_hover_text("Save this setup (Ctrl+S)").clicked() {
                    self.save();
                }
                let busy = self.import.is_some();
                let label = if busy { "Choosing a preset…" } else { "Open Razer preset…" };
                if ui
                    .add_enabled(!busy, egui::Button::new(label))
                    .on_hover_text("Import a Razer Chroma Studio .ChromaEffects file. It replaces the layers shown here.")
                    .clicked()
                {
                    self.open_preset(ui.ctx());
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Brightness");
            ui.add(Slider::new(&mut self.setup.brightness, 0..=100).suffix("%"));
            ui.add_space(18.0);
            ui.label("Frame rate");
            ui.add(Slider::new(&mut self.setup.fps, 10..=60).suffix(" fps"))
                .on_hover_text("How often moving effects are redrawn. Lower uses less power.");
        });
        self.notice_banner(ui);
        ui.add_space(4.0);
    }

    fn setup_menu(&mut self, ui: &mut egui::Ui, unsaved: bool) {
        ui.menu_button("Setup ▾", |ui| {
            self.refresh_names();
            ui.label(RichText::new("Saved setups").weak());
            if self.saved_names.is_empty() {
                ui.label(RichText::new("None yet").weak().italics());
            }
            let mut load = None;
            for name in &self.saved_names {
                let current = *name == self.setup.name;
                let resp = ui.selectable_label(current, name);
                let resp = if current { resp.on_hover_text("Load the saved version again") } else { resp };
                if resp.clicked() {
                    load = Some(name.clone());
                }
            }
            if let Some(name) = load {
                ui.close();
                self.request(Pending::Load(name));
            }
            ui.separator();
            if ui.add_enabled(unsaved, egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
                ui.close();
                self.save();
            }
            if ui.button("Save as…").clicked() {
                ui.close();
                self.dialog = Some(Dialog::SaveAs(self.setup.name.clone()));
            }
            if ui.button("Rename…").clicked() {
                ui.close();
                self.dialog = Some(Dialog::Rename(self.setup.name.clone()));
            }
            let has_file = self.saved_names.contains(&self.setup.name);
            if ui.add_enabled(has_file, egui::Button::new("Delete saved setup…")).clicked() {
                ui.close();
                self.dialog = Some(Dialog::ConfirmDelete);
            }
            ui.separator();
            if ui.button("Start from the default setup").clicked() {
                ui.close();
                self.request(Pending::OwnerDefault);
            }
        });
    }

    fn notice_banner(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = &self.notice else { return };
        let (text, color) = match notice {
            Notice::Info(t) => (t.as_str(), ui.visuals().text_color()),
            Notice::Error(t) => (t.as_str(), RED),
        };
        let mut dismiss = false;
        egui::Frame::group(ui.style()).fill(ui.visuals().faint_bg_color).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                dismiss = ui.small_button("×").on_hover_text("Dismiss").clicked();
                ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
            });
        });
        if dismiss {
            self.notice = None;
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (color, text) = match self.daemon {
                DaemonState::Ready => (GREEN, "Lighting is running and the keyboard is connected."),
                DaemonState::NoKeyboard => (AMBER, "Lighting is running, but the keyboard was not found."),
                DaemonState::NotRunning => {
                    (RED, "The lighting program (keyglowd) is not running. Changes are kept and show once it starts.")
                }
            };
            ui.label(RichText::new("●").color(color));
            ui.label(text);
            if self.daemon != DaemonState::NotRunning && !self.daemon_detail.is_empty() {
                ui.label(RichText::new(&self.daemon_detail).weak());
            }
        });
    }

    fn layer_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("Layers");
        ui.horizontal(|ui| {
            ui.menu_button("+ Add", |ui| {
                for kind in EffectKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        ui.close();
                        let i = model::add_layer(&mut self.setup, kind);
                        self.select(Some(i));
                    }
                }
            })
            .response
            .on_hover_text("Add a layer on top");
            let sel = self.selected.filter(|&i| i < self.setup.layers.len());
            let n = self.setup.layers.len();
            if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                if let Some(i) = sel {
                    let next = model::delete_layer(&mut self.setup, i);
                    self.select(next);
                }
            }
            let up = sel.is_some_and(|i| i + 1 < n);
            if ui.add_enabled(up, egui::Button::new("↑")).on_hover_text("Move up").clicked() {
                self.selected = sel.map(|i| model::move_up(&mut self.setup, i));
            }
            let down = sel.is_some_and(|i| i > 0);
            if ui.add_enabled(down, egui::Button::new("↓")).on_hover_text("Move down").clicked() {
                self.selected = sel.map(|i| model::move_down(&mut self.setup, i));
            }
        });
        ui.separator();
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(RichText::new("Layers higher in the list are drawn over the ones below.").weak().small());
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if self.setup.layers.is_empty() {
                        ui.label(RichText::new("No layers yet. Add one to light the keyboard.").weak());
                    }
                    for i in model::display_order(&self.setup) {
                        self.layer_row(ui, i);
                    }
                });
            });
        });
    }

    /// One line of the layer list: on/off checkbox, effect name, colour chips, opacity.
    fn layer_row(&mut self, ui: &mut egui::Ui, i: usize) {
        let selected = self.selected == Some(i);
        let mut clicked = false;
        ui.horizontal(|ui| {
            let layer = &mut self.setup.layers[i];
            ui.checkbox(&mut layer.enabled, "").on_hover_text("Turn this layer on or off");
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::click());
            clicked = resp.clicked();
            if !ui.is_rect_visible(rect) {
                return;
            }
            let visuals = ui.visuals();
            if selected {
                ui.painter().rect_filled(rect, 4.0, visuals.selection.bg_fill);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
            }
            let text_color = match (selected, layer.enabled) {
                (true, _) => visuals.selection.stroke.color,
                (false, true) => visuals.text_color(),
                (false, false) => visuals.weak_text_color(),
            };
            let name = ui.painter().text(
                rect.left_center() + egui::vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                layer.effect.kind().label(),
                FontId::proportional(14.0),
                text_color,
            );
            let opacity = ui.painter().text(
                rect.right_center() - egui::vec2(8.0, 0.0),
                Align2::RIGHT_CENTER,
                format!("{}%", layer.opacity),
                FontId::proportional(12.0),
                visuals.weak_text_color(),
            );
            // Colour chips between the name and the opacity, as many as fit.
            let mut x = name.right() + 10.0;
            let limit = opacity.left() - 8.0;
            match editors::preview(&layer.effect) {
                Preview::Colors(colors) => {
                    for c in colors {
                        if x + 10.0 > limit {
                            break;
                        }
                        let chip =
                            egui::Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), egui::vec2(10.0, 10.0));
                        let c = if layer.enabled { c } else { c.gamma_multiply(0.35) };
                        ui.painter().rect_filled(chip, 2.0, c);
                        x += 12.0;
                    }
                }
                Preview::Random => {
                    if x + 40.0 < limit {
                        ui.painter().text(
                            egui::pos2(x, rect.center().y),
                            Align2::LEFT_CENTER,
                            "random",
                            FontId::proportional(12.0),
                            visuals.weak_text_color(),
                        );
                    }
                }
            }
        });
        if clicked {
            self.select(Some(i));
        }
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.selected.filter(|&i| i < self.setup.layers.len()) else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Select a layer on the left, or add one.").weak());
            });
            return;
        };
        let n = self.setup.layers.len();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading(format!("{} layer", self.setup.layers[i].effect.kind().label()));
            ui.label(RichText::new(format!("{} of {n} from the top", n - i)).weak());
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            editors::layer_editor(ui, &mut self.setup.layers[i], &mut self.color_memory);
        });
    }

    // ---- Dialogs --------------------------------------------------------------------------

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.take() else { return };
        self.dialog = match dialog {
            Dialog::SaveAs(name) => self.name_dialog(ctx, name, true),
            Dialog::Rename(name) => self.name_dialog(ctx, name, false),
            Dialog::ConfirmDelete => self.delete_dialog(ctx),
            Dialog::Discard(pending) => self.discard_dialog(ctx, pending),
        };
    }

    /// Save as (`save_as`) or Rename. Returns the dialog while it stays open.
    fn name_dialog(&mut self, ctx: &egui::Context, mut name: String, save_as: bool) -> Option<Dialog> {
        let mut outcome = Outcome::Open;
        let current = self.setup.name.clone();
        let names = self.saved_names.clone();
        let resp = egui::Modal::new(Id::unique("name-dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(if save_as { "Save as" } else { "Rename setup" });
            ui.add_space(4.0);
            let edit =
                ui.add(egui::TextEdit::singleline(&mut name).hint_text("Setup name").desired_width(f32::INFINITY));
            if ui.memory(|m| m.focused().is_none()) {
                edit.request_focus();
            }
            let trimmed = name.trim().to_owned();
            let exists = names.contains(&trimmed) && trimmed != current;
            let check = model::check_setup_name(&trimmed);
            let ok = match &check {
                Err(why) => {
                    ui.label(RichText::new(why).color(RED));
                    false
                }
                Ok(()) if exists && save_as => {
                    ui.label(RichText::new(format!("This replaces the saved setup “{trimmed}”.")).color(AMBER));
                    true
                }
                Ok(()) if exists => {
                    ui.label(RichText::new(format!("A saved setup named “{trimmed}” already exists.")).color(RED));
                    false
                }
                Ok(()) => {
                    ui.label("");
                    true
                }
            };
            ui.add_space(4.0);
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                let label = match (save_as, exists) {
                    (true, true) => "Replace",
                    (true, false) => "Save",
                    (false, _) => "Rename",
                };
                if ui.add_enabled(ok, egui::Button::new(label)).clicked() || (ok && enter) {
                    let done = if save_as { self.save_as(trimmed.clone()) } else { self.rename(trimmed.clone()) };
                    if done {
                        outcome = Outcome::Close;
                    }
                }
                if ui.button("Cancel").clicked() {
                    outcome = Outcome::Close;
                }
            });
        });
        if resp.should_close() {
            outcome = Outcome::Close;
        }
        match outcome {
            Outcome::Open if save_as => Some(Dialog::SaveAs(name)),
            Outcome::Open => Some(Dialog::Rename(name)),
            Outcome::Close => None,
        }
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) -> Option<Dialog> {
        let mut outcome = Outcome::Open;
        let resp = egui::Modal::new(Id::unique("delete-dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Delete saved setup");
            ui.label(format!(
                "Delete the saved setup “{}”? The keyboard keeps showing it until you switch setups.",
                self.setup.name
            ));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Delete").color(RED)).clicked() {
                    self.delete_saved();
                    outcome = Outcome::Close;
                }
                if ui.button("Cancel").clicked() {
                    outcome = Outcome::Close;
                }
            });
        });
        if resp.should_close() {
            outcome = Outcome::Close;
        }
        match outcome {
            Outcome::Open => Some(Dialog::ConfirmDelete),
            Outcome::Close => None,
        }
    }

    /// Asks before replacing a setup that has unsaved edits.
    fn discard_dialog(&mut self, ctx: &egui::Context, pending: Pending) -> Option<Dialog> {
        enum Choice {
            Save,
            Discard,
            Cancel,
        }
        let mut choice = None;
        let resp = egui::Modal::new(Id::unique("discard-dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Unsaved changes");
            ui.label(format!("“{}” has changes that are not saved. Save them before switching?", self.setup.name));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Save, then switch").clicked() {
                    choice = Some(Choice::Save);
                }
                if ui.button("Discard changes").clicked() {
                    choice = Some(Choice::Discard);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
        if resp.should_close() && choice.is_none() {
            choice = Some(Choice::Cancel);
        }
        match choice {
            None => Some(Dialog::Discard(pending)),
            Some(Choice::Cancel) => None,
            Some(Choice::Discard) => {
                self.apply(pending);
                None
            }
            // A failed save shows its error and keeps the question open.
            Some(Choice::Save) if !self.save() => Some(Dialog::Discard(pending)),
            Some(Choice::Save) => {
                self.apply(pending);
                None
            }
        }
    }

    // ---- The frame ------------------------------------------------------------------------

    /// One frame of the whole window.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let before = self.setup.clone();
        self.poll_import();
        self.refresh_status(now);

        let save_key = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        if self.dialog.is_none() && ui.input_mut(|i| i.consume_shortcut(&save_key)) {
            self.save();
        }

        egui::Panel::top("setup-bar").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(2.0);
            self.status_bar(ui);
            ui.add_space(2.0);
        });
        egui::Panel::left("layers").default_size(260.0).size_range(200.0..=420.0).show(ui, |ui| self.layer_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.settings_panel(ui));
        self.dialogs(ui.ctx());

        if self.setup != before {
            self.dirty = true;
        }
        // Idle unless something is waiting: the next status read, or a throttled write.
        let mut wake = STATUS_EVERY;
        if let Some(wait) = self.autosave(now) {
            wake = wake.min(wait.max(Duration::from_millis(1)));
        }
        ui.ctx().request_repaint_after(wake);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush();
    }
}

/// Reads a preset file (refusing huge ones without reading them) and imports it.
fn read_preset(path: &Path) -> Result<Imported, String> {
    let shown = path.display();
    let file = std::fs::File::open(path).map_err(|e| format!("Could not open {shown}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_PRESET_FILE + 1).read_to_end(&mut bytes).map_err(|e| format!("Could not read {shown}: {e}"))?;
    if bytes.len() as u64 > MAX_PRESET_FILE {
        return Err(preset::PresetError::TooLarge.to_string());
    }
    let hint = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    preset::import(&bytes, &hint).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyglow_core::setup::Effect;

    fn app() -> (tempfile::TempDir, App) {
        let d = tempfile::tempdir().unwrap();
        let store = Store::new(d.path().join("keyglow"));
        let app = App::open(store, d.path().join("run").join("status.json"));
        (d, app)
    }

    fn active_on_disk(app: &App) -> Option<String> {
        std::fs::read_to_string(app.store.active_path()).ok()
    }

    #[test]
    fn a_broken_active_file_shows_an_error_and_is_left_alone_until_a_change() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::new(d.path().join("keyglow"));
        std::fs::create_dir_all(d.path().join("keyglow")).unwrap();
        std::fs::write(store.active_path(), "{ broken").unwrap();
        let mut app = App::open(store, d.path().join("status.json"));
        assert!(matches!(app.notice, Some(Notice::Error(_))));
        assert_eq!(app.setup, Setup::owner_default());
        let t0 = Instant::now();
        assert_eq!(app.autosave(t0), None);
        app.flush();
        assert_eq!(active_on_disk(&app).as_deref(), Some("{ broken"));
        // The first change writes it.
        app.setup.brightness = 50;
        app.dirty = true;
        assert_eq!(app.autosave(t0), None);
        assert_eq!(app.store.load_active().unwrap().brightness, 50);
    }

    #[test]
    fn edits_reach_active_json_but_saved_setups_only_change_on_save() {
        let (_d, mut app) = app();
        assert!(app.is_unsaved());
        assert!(app.save());
        assert!(!app.is_unsaved());
        assert_eq!(app.saved_names, vec!["Default"]);

        let t0 = Instant::now();
        app.setup.layers[0].opacity = 40;
        app.dirty = true;
        assert_eq!(app.autosave(t0), None);
        app.setup.layers[0].opacity = 30;
        app.dirty = true;
        // Inside the 50 ms gap: waits, and says how long.
        assert_eq!(app.autosave(t0 + Duration::from_millis(20)), Some(Duration::from_millis(30)));
        assert_eq!(app.store.load_active().unwrap().layers[0].opacity, 40);
        assert_eq!(app.autosave(t0 + Duration::from_millis(50)), None);
        assert_eq!(app.store.load_active().unwrap().layers[0].opacity, 30);
        // The saved file still has the old value.
        assert_eq!(app.store.load_setup("Default").unwrap().layers[0].opacity, 100);
        assert!(app.is_unsaved());
        assert!(app.save());
        assert_eq!(app.store.load_setup("Default").unwrap().layers[0].opacity, 30);
    }

    #[test]
    fn save_as_rename_and_delete() {
        let (_d, mut app) = app();
        assert!(app.save_as("Night".into()));
        assert_eq!(app.setup.name, "Night");
        assert_eq!(app.saved_names, vec!["Night"]);
        assert!(!app.is_unsaved());
        assert!(app.dirty, "the new name goes to active.json too");

        assert!(app.rename("Evening".into()));
        assert_eq!(app.saved_names, vec!["Evening"]);
        assert!(!app.is_unsaved());

        // Renaming onto another saved setup is refused.
        assert!(app.save_as("Other".into()));
        assert!(!app.rename("Evening".into()));
        assert_eq!(app.setup.name, "Other");
        assert!(matches!(app.notice, Some(Notice::Error(_))));

        // A bad name is refused and the old name kept.
        assert!(!app.save_as("a/b".into()));
        assert_eq!(app.setup.name, "Other");

        app.delete_saved();
        assert_eq!(app.saved_names, vec!["Evening"]);
        assert!(app.is_unsaved());
    }

    #[test]
    fn switching_away_from_edits_asks_first() {
        let (_d, mut app) = app();
        let mut other = Setup::owner_default();
        other.name = "Other".into();
        other.layers.truncate(1);
        app.store.save_setup(&other).unwrap();

        // The untouched default switches straight away.
        app.request(Pending::Load("Other".into()));
        assert!(app.dialog.is_none());
        assert_eq!(app.setup, other);
        assert!(!app.is_unsaved());
        assert_eq!(app.selected, Some(0));

        // An edited setup asks first, and nothing changes until answered.
        app.setup.brightness = 10;
        app.request(Pending::OwnerDefault);
        assert!(matches!(app.dialog, Some(Dialog::Discard(_))));
        assert_eq!(app.setup.brightness, 10);
        let Some(Dialog::Discard(pending)) = app.dialog.take() else { unreachable!() };
        app.apply(pending);
        assert_eq!(app.setup, Setup::owner_default());
    }

    #[test]
    fn an_imported_preset_replaces_the_setup_and_reports_what_was_skipped() {
        let (_d, mut app) = app();
        let mut setup = Setup::owner_default();
        setup.name = "Blue Pink".into();
        setup.layers = vec![keyglow_core::setup::Layer {
            enabled: true,
            opacity: 80,
            effect: Effect::default_for(EffectKind::Static),
        }];
        app.imported(Imported {
            setup: setup.clone(),
            skipped: vec!["Audio Meter".into()],
            notes: vec!["A note.".into()],
        });
        assert_eq!(app.setup, setup);
        assert!(app.dirty);
        match &app.notice {
            Some(Notice::Info(t)) => {
                assert!(
                    t.contains("Blue Pink")
                        && t.contains("1 layer.")
                        && t.contains("Audio Meter")
                        && t.contains("A note."),
                    "{t}"
                );
            }
            _ => panic!("expected an info notice"),
        }
    }

    #[test]
    fn a_missing_preset_file_is_an_error_message() {
        let d = tempfile::tempdir().unwrap();
        assert!(read_preset(&d.path().join("nope.ChromaEffects")).unwrap_err().contains("Could not open"));
        let junk = d.path().join("junk.ChromaEffects");
        std::fs::write(&junk, b"not a zip").unwrap();
        assert_eq!(read_preset(&junk).unwrap_err(), preset::PresetError::NotAZip.to_string());
    }

    /// Runs frames of the real drawing code in a headless egui context (no window, no GPU).
    fn frames(ctx: &egui::Context, app: &mut App, n: usize) -> Duration {
        let mut delay = Duration::ZERO;
        for _ in 0..n {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 580.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.draw(ui));
            delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            out.drop_without_applying_deltas();
        }
        delay
    }

    #[test]
    fn every_editor_and_dialog_draws_and_the_window_goes_idle() {
        let (_d, mut app) = app();
        let ctx = egui::Context::default();
        configure(&ctx);
        // With nothing happening, the only wake-up asked for is the status line's, once a second.
        // The colour picker makes the panel scroll at this size, and the scroll bar fades in
        // over egui's 0.2 s animation; a headless frame moves time on by 1/60 s, so give it 20.
        let delay = frames(&ctx, &mut app, 20);
        assert!(delay >= Duration::from_millis(900), "repaint delay {delay:?}");
        assert!(!app.dirty);
        assert!(app.store.load_active().is_ok());
        assert!(active_on_disk(&app).is_none(), "nothing changed, so nothing was written");

        for kind in EffectKind::ALL {
            let i = model::add_layer(&mut app.setup, kind);
            app.select(Some(i));
            frames(&ctx, &mut app, 2);
        }
        // A palette switched to Random, an empty setup, and a notice.
        app.setup.layers[1].effect = Effect::default_for(EffectKind::Starlight);
        app.notice = Some(Notice::Info("Imported.\nA second line.".into()));
        frames(&ctx, &mut app, 2);
        for dialog in [
            Dialog::SaveAs("Night".into()),
            Dialog::Rename("a/b".into()),
            Dialog::ConfirmDelete,
            Dialog::Discard(Pending::OwnerDefault),
        ] {
            app.dialog = Some(dialog);
            frames(&ctx, &mut app, 2);
            assert!(app.dialog.is_some(), "a dialog stays open until answered");
            app.dialog = None;
        }
        app.setup.layers.clear();
        app.select(None);
        // Edits made here, outside a frame, are not seen by the frame's own change check, so
        // mark them; the next frame after the throttle gap writes them.
        app.dirty = true;
        std::thread::sleep(Duration::from_millis(60));
        frames(&ctx, &mut app, 1);
        assert!(!app.dirty);
        assert!(app.store.load_active().unwrap().layers.is_empty());
    }

    /// Pairs of clickable widgets drawn over each other in the last frame. A widget lying wholly
    /// inside another (a button in a scroll area) is how containers work, so only partial
    /// overlaps count.
    fn overlapping_widgets(ctx: &egui::Context) -> Vec<(egui::Rect, egui::Rect)> {
        let widgets = ctx.viewport(|vp| vp.prev_pass.widgets.clone());
        let mut found = Vec::new();
        for (_, rects) in widgets.layers() {
            let clickable: Vec<egui::Rect> = rects
                .iter()
                .filter(|w| w.visible && (w.sense.senses_click() || w.sense.senses_drag()))
                .map(|w| w.interact_rect)
                .filter(|r| r.is_positive())
                .collect();
            for (i, &a) in clickable.iter().enumerate() {
                for &b in &clickable[..i] {
                    if a.intersect(b).area() > 1.0 && !a.contains_rect(b) && !b.contains_rect(a) {
                        found.push((a, b));
                    }
                }
            }
        }
        found
    }

    #[test]
    fn no_editor_draws_widgets_over_each_other() {
        let (_d, mut app) = app();
        let ctx = egui::Context::default();
        configure(&ctx);
        app.setup.layers.clear();
        for kind in EffectKind::ALL {
            let i = model::add_layer(&mut app.setup, kind);
            app.select(Some(i));
            // The first frames size new grids; check once the layout has settled.
            frames(&ctx, &mut app, 3);
            let overlaps = overlapping_widgets(&ctx);
            assert!(overlaps.is_empty(), "{} editor: {overlaps:?}", kind.label());
        }
    }

    #[test]
    fn status_without_a_status_file_is_not_running() {
        let (_d, mut app) = app();
        app.refresh_status(Instant::now());
        assert_eq!(app.daemon, DaemonState::NotRunning);
    }
}
