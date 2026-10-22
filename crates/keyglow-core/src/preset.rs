//! Import of Razer Chroma Studio `.ChromaEffects` presets (a zip holding one XML file).
//!
//! Presets come from the internet, so everything here is bounded and in memory: no file is
//! ever written, sizes are capped even when the zip lies about them, the XML tree has a depth
//! and node cap, and every failure is a `PresetError` rather than a panic.

use crate::color::Rgb;
use crate::setup::{Effect, EffectKind, Layer, Palette, Setup};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fmt;
use std::io::Read;

const MAX_INPUT: usize = 5 * 1024 * 1024;
const MAX_XML: u64 = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 200_000;
const MAX_LAYERS: usize = 64;
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug)]
pub struct Imported {
    pub setup: Setup,
    /// Razer effect names Keyglow doesn't have (or can't do in this form).
    pub skipped: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PresetError {
    NotAZip,
    NoXml,
    TooLarge,
    BadXml(String),
}

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PresetError::NotAZip => write!(f, "This file is not a Razer preset (it is not a zip file)."),
            PresetError::NoXml => write!(f, "This preset has no XML file inside it."),
            PresetError::TooLarge => write!(f, "This preset is too large to import safely."),
            PresetError::BadXml(why) => write!(f, "This preset could not be read: {why}"),
        }
    }
}

impl std::error::Error for PresetError {}

/// A tiny XML tree. Bounded by `MAX_DEPTH` and `MAX_NODES` while building.
#[derive(Default)]
struct Node {
    name: String,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }
    fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.trim())
    }
    fn count_named(&self, name: &str) -> usize {
        usize::from(self.name == name) + self.children.iter().map(|c| c.count_named(name)).sum::<usize>()
    }
}

fn parse_tree(xml: &[u8]) -> Result<Node, PresetError> {
    let bad = |m: String| PresetError::BadXml(m);
    let mut reader = Reader::from_reader(xml);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut nodes = 0usize;

    fn attach(n: Node, stack: &mut [Node], root: &mut Option<Node>) -> Result<(), PresetError> {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(n);
        } else if root.is_none() {
            *root = Some(n);
        } else {
            return Err(PresetError::BadXml("more than one root element".into()));
        }
        Ok(())
    }
    let new_node = |e: &quick_xml::events::BytesStart<'_>, depth: usize, nodes: &mut usize| {
        *nodes += 1;
        if depth >= MAX_DEPTH {
            return Err(PresetError::BadXml("elements are nested too deeply".into()));
        }
        if *nodes > MAX_NODES {
            return Err(PresetError::BadXml("too many elements".into()));
        }
        Ok(Node { name: AsRef::<str>::as_ref(&e.local_name()).to_owned(), ..Node::default() })
    };

    loop {
        match reader.read_event().map_err(|e| bad(e.to_string()))? {
            Event::Start(e) => {
                let n = new_node(&e, stack.len(), &mut nodes)?;
                stack.push(n);
            }
            Event::Empty(e) => {
                let n = new_node(&e, stack.len(), &mut nodes)?;
                attach(n, &mut stack, &mut root)?;
            }
            Event::End(_) => {
                let n = stack.pop().ok_or_else(|| bad("unbalanced closing tag".into()))?;
                attach(n, &mut stack, &mut root)?;
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t);
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&c);
                }
            }
            Event::GeneralRef(r) => {
                // Only the five predefined entities and numeric references are honoured; a
                // preset can't define its own entities (DOCTYPE is refused below).
                if let Some(top) = stack.last_mut() {
                    if let Ok(Some(ch)) = r.resolve_char_ref() {
                        top.text.push(ch);
                    } else if let Some(s) = quick_xml::escape::resolve_predefined_entity(&r) {
                        top.text.push_str(s);
                    }
                }
            }
            Event::DocType(_) => return Err(bad("a DOCTYPE is not allowed".into())),
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(bad("the file ends before all elements are closed".into()));
    }
    root.ok_or_else(|| bad("the file has no elements".into()))
}

/// Maps a Razer `Speed` (about 1 to 30 in Chroma Studio) onto Keyglow's 1 to 100 scale.
/// Keyglow's own defaults sit at 30 to 50, which is where Chroma Studio's middle setting
/// (about 10 to 15) lands when multiplied by 3, so Razer 10 gives 30, Razer 20 gives 60 and
/// Razer 30 and above gives 90 or more. The result is clamped to 1..=100.
fn map_speed(raw: f64) -> u8 {
    (raw * 3.0).round().clamp(1.0, 100.0) as u8
}

fn num(text: Option<&str>) -> Option<f64> {
    text.and_then(|t| t.parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn channel(c: &Node, name: &str) -> u8 {
    num(c.child_text(name)).unwrap_or(0.0).clamp(0.0, 255.0).round() as u8
}

fn colors_of(region: &Node) -> Vec<Rgb> {
    let Some(cols) = region.child("Colors") else { return Vec::new() };
    cols.children_named("RzColor")
        .take(16)
        .map(|c| Rgb { r: channel(c, "Red"), g: channel(c, "Green"), b: channel(c, "Blue") })
        .collect()
}

fn palette_for(kind: EffectKind, colors: &[Rgb], random: bool) -> Option<Palette> {
    if !random && !colors.is_empty() {
        return Some(Palette::Colors(colors.to_vec()));
    }
    match kind {
        EffectKind::Wave | EffectKind::Wheel => Some(Palette::Rainbow),
        EffectKind::Fire => None,
        _ => Some(Palette::Random),
    }
}

fn kind_of(name: &str) -> Option<EffectKind> {
    Some(match name.to_ascii_lowercase().as_str() {
        "static" => EffectKind::Static,
        "breathing" => EffectKind::Breathing,
        "spectrum" => EffectKind::Spectrum,
        "wave" => EffectKind::Wave,
        "wheel" => EffectKind::Wheel,
        "starlight" => EffectKind::Starlight,
        "reactive" => EffectKind::Reactive,
        "ripple" => EffectKind::Ripple,
        "fire" => EffectKind::Fire,
        _ => return None,
    })
}

fn build_effect(kind: EffectKind, region: &Node, random: bool) -> Effect {
    let colors = colors_of(region);
    let speed = num(region.child_text("Speed")).map(map_speed);
    let mut effect = Effect::default_for(kind);
    let new_palette = palette_for(kind, &colors, random);
    match &mut effect {
        Effect::Static { color } => {
            if let Some(c) = colors.first() {
                *color = *c;
            }
        }
        Effect::Breathing { palette, speed: s } | Effect::Wheel { palette, speed: s, .. } => {
            set(palette, new_palette);
            set_speed(s, speed);
        }
        Effect::Spectrum { speed: s } => set_speed(s, speed),
        Effect::Wave { palette, speed: s, .. } => {
            set(palette, new_palette);
            set_speed(s, speed);
        }
        Effect::Starlight { palette, density, duration_ms } => {
            set(palette, new_palette);
            if let Some(d) = num(region.child_text("Density")) {
                const DENSITY: [u8; 5] = [15, 30, 50, 70, 90];
                *density = DENSITY[(d.round().clamp(1.0, 5.0) as usize) - 1];
            }
            match region.child_text("Duration").map(str::to_ascii_lowercase).as_deref() {
                Some("short") => *duration_ms = 600,
                Some("medium") => *duration_ms = 1200,
                Some("long") => *duration_ms = 2400,
                _ => {}
            }
        }
        Effect::Reactive { palette, .. } => set(palette, new_palette),
        Effect::Ripple { palette, speed: s, reach, .. } => {
            set(palette, new_palette);
            set_speed(s, speed);
            // Chroma Studio's Width is in percent-like units (300 is a wide ring). Keyglow's
            // reach is in keys, so 25 units per key; narrow widths keep the default reach.
            if let Some(w) = num(region.child_text("Width")).filter(|w| *w > 25.0) {
                *reach = (w / 25.0).round().clamp(1.0, 36.0) as u8;
            }
        }
        Effect::Fire { palette, speed: s, .. } => {
            set(palette, new_palette);
            set_speed(s, speed);
        }
    }
    effect
}

fn set(slot: &mut Palette, new: Option<Palette>) {
    if let Some(p) = new {
        *slot = p;
    }
}

fn set_speed(slot: &mut u8, new: Option<u8>) {
    if let Some(s) = new {
        *slot = s;
    }
}

fn clean_name(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(MAX_NAME_CHARS).collect::<String>().trim().to_string()
}

fn name_from_hint(hint: &str) -> String {
    let stem = std::path::Path::new(hint).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let n = clean_name(stem);
    if n.is_empty() { "Imported preset".into() } else { n }
}

fn read_xml(bytes: &[u8]) -> Result<Vec<u8>, PresetError> {
    if bytes.len() > MAX_INPUT {
        return Err(PresetError::TooLarge);
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| PresetError::NotAZip)?;
    let mut found = None;
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index_raw(i) {
            if f.is_file() && f.name().is_ok_and(|n| n.to_ascii_lowercase().ends_with(".xml")) {
                found = Some(i);
                break;
            }
        }
    }
    let i = found.ok_or(PresetError::NoXml)?;
    let file = archive.by_index(i).map_err(|e| PresetError::BadXml(e.to_string()))?;
    if file.size() > MAX_XML {
        return Err(PresetError::TooLarge);
    }
    // The declared size can lie, so also stop reading past the limit.
    let mut buf = Vec::new();
    file.take(MAX_XML + 1).read_to_end(&mut buf).map_err(|e| PresetError::BadXml(e.to_string()))?;
    if buf.len() as u64 > MAX_XML {
        return Err(PresetError::TooLarge);
    }
    Ok(buf)
}

pub fn import(bytes: &[u8], name_hint: &str) -> Result<Imported, PresetError> {
    let xml = read_xml(bytes)?;
    let root = parse_tree(&xml)?;
    if root.name != "LightingEffects" {
        return Err(PresetError::BadXml("this is not a Chroma Studio lighting file".into()));
    }
    let random = root.child_text("RandomColor").is_some_and(|t| t.eq_ignore_ascii_case("true"));
    let empty = Node::default();
    let layers_xml: Vec<&Node> = root.child("EffectLayers").map(|l| l.children_named("EffectLayer").collect()).unwrap_or_default();
    let max_cells = layers_xml.iter().map(|l| l.count_named("DeviceCell")).max().unwrap_or(0);

    let mut layers = Vec::new();
    let mut skipped = Vec::new();
    let mut notes = Vec::new();
    for (n, lx) in layers_xml.iter().enumerate() {
        if n >= MAX_LAYERS {
            notes.push(format!("Only the first {MAX_LAYERS} layers were imported."));
            break;
        }
        let raw_name = lx.child_text("Effect").unwrap_or("");
        let shown: String = clean_name(raw_name).chars().take(40).collect();
        let region = lx.child("Regions").and_then(|r| r.child("EffectRegion")).unwrap_or(&empty);
        let Some(kind) = kind_of(raw_name) else {
            skipped.push(if shown.is_empty() { "(unnamed effect)".into() } else { shown });
            continue;
        };
        let label = lx.child_text("Name").map(clean_name).filter(|s| !s.is_empty()).unwrap_or_else(|| kind.label().to_string());
        let on_key = region.child_text("Start").is_some_and(|s| s.eq_ignore_ascii_case("onkeypress"));
        let effect = if !on_key || matches!(kind, EffectKind::Ripple | EffectKind::Reactive) {
            build_effect(kind, region, random)
        } else if matches!(kind, EffectKind::Static | EffectKind::Breathing) {
            let colors = colors_of(region);
            let palette = match kind {
                EffectKind::Static => Palette::Colors(vec![colors.first().copied().unwrap_or(Rgb::WHITE)]),
                _ => palette_for(kind, &colors, random).unwrap_or(Palette::Random),
            };
            notes.push(format!("{label} starts on a key press, so it became a Reactive layer."));
            let mut e = Effect::default_for(EffectKind::Reactive);
            if let Effect::Reactive { palette: p, .. } = &mut e {
                *p = palette;
            }
            e
        } else {
            skipped.push(format!("{shown} (on key press)"));
            continue;
        };
        let cells = lx.count_named("DeviceCell");
        if cells < max_cells {
            notes.push(format!("{label} covered part of the keyboard; here it covers all of it"));
        }
        let enabled = !lx.child_text("Visible").is_some_and(|t| t.eq_ignore_ascii_case("false"));
        let opacity = num(lx.child_text("Alpha")).unwrap_or(100.0).clamp(0.0, 100.0).round() as u8;
        layers.push(Layer { enabled, opacity, effect });
    }

    let name = root.child_text("Name").map(clean_name).filter(|s| !s.is_empty()).unwrap_or_else(|| name_from_hint(name_hint));
    let setup = Setup { name, fps: 30, brightness: 100, layers }.sanitized();
    Ok(Imported { setup, skipped, notes })
}
