use keyglow_core::color::Rgb;
use keyglow_core::preset::{import, PresetError};
use keyglow_core::setup::{Effect, Palette};
use std::io::Write;
use zip::write::SimpleFileOptions;

const SAMPLE: &str = include_str!("fixtures/sample.xml");

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn layer(effect: &str, start: &str, colors: &[(u8, u8, u8)], extra: &str, cells: usize) -> String {
    let cols: String = colors
        .iter()
        .map(|(r, g, b)| format!("<RzColor><Red>{r}</Red><Green>{g}</Green><Blue>{b}</Blue></RzColor>"))
        .collect();
    let cells = "<DeviceCell><Row>0</Row><Col>0</Col></DeviceCell>".repeat(cells);
    format!(
        "<EffectLayer><Effect>{effect}</Effect><Regions><EffectRegion><Start>{start}</Start>\
         <Colors>{cols}</Colors>{extra}<Cells>{cells}</Cells></EffectRegion></Regions>\
         <Visible>true</Visible><Alpha>100</Alpha><Name>{effect}</Name></EffectLayer>"
    )
}

fn preset(layers: &[String]) -> Vec<u8> {
    let xml = format!(
        "<?xml version=\"1.0\"?><LightingEffects><Colors /><RandomColor>false</RandomColor>\
         <EffectLayers>{}</EffectLayers><Name>Mine</Name></LightingEffects>",
        layers.concat()
    );
    zip_of(&[("a.xml", xml.as_bytes())])
}

#[test]
fn imports_the_owners_sample() {
    let zip = zip_of(&[("preset.xml", SAMPLE.as_bytes())]);
    let got = import(&zip, "whatever.ChromaEffects").unwrap();
    assert_eq!(got.setup.name, "Blue Pink StarLight White Ripple");
    assert!(got.skipped.is_empty());
    assert_eq!(got.setup.layers.len(), 3);
    for l in &got.setup.layers {
        assert!(l.enabled);
        assert_eq!(l.opacity, 100);
    }
    assert_eq!(got.setup.layers[0].effect, Effect::Static { color: Rgb { r: 0, g: 255, b: 255 } });
    match &got.setup.layers[1].effect {
        Effect::Starlight { palette, density, duration_ms } => {
            assert_eq!(palette, &Palette::Colors(vec![Rgb { r: 247, g: 1, b: 255 }]));
            assert_eq!(*duration_ms, 2400);
            assert_eq!(*density, 50);
        }
        other => panic!("{other:?}"),
    }
    match &got.setup.layers[2].effect {
        Effect::Ripple { palette, reach, .. } => {
            assert_eq!(palette, &Palette::Colors(vec![Rgb::WHITE]));
            assert_eq!(*reach, 12);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_effects_are_skipped_by_name() {
    let zip = preset(&[
        layer("static", "auto", &[(1, 2, 3)], "", 0),
        layer("audiometer", "auto", &[], "", 0),
        layer("spectrum", "auto", &[], "<Speed>10</Speed>", 0),
    ]);
    let got = import(&zip, "x.ChromaEffects").unwrap();
    assert_eq!(got.skipped, vec!["audiometer".to_string()]);
    assert_eq!(got.setup.layers.len(), 2);
    assert_eq!(got.setup.name, "Mine");
}

#[test]
fn key_press_static_becomes_reactive() {
    let zip = preset(&[layer("static", "onkeypress", &[(9, 8, 7)], "", 0)]);
    let got = import(&zip, "x").unwrap();
    assert_eq!(got.setup.layers.len(), 1);
    match &got.setup.layers[0].effect {
        Effect::Reactive { palette, .. } => assert_eq!(palette, &Palette::Colors(vec![Rgb { r: 9, g: 8, b: 7 }])),
        other => panic!("{other:?}"),
    }
    assert_eq!(got.notes.len(), 1);
    assert!(got.skipped.is_empty());
}

#[test]
fn key_press_wave_is_skipped() {
    let zip = preset(&[layer("wave", "onkeypress", &[(1, 1, 1)], "", 0)]);
    let got = import(&zip, "x").unwrap();
    assert_eq!(got.skipped, vec!["wave (on key press)".to_string()]);
    assert!(got.setup.layers.is_empty());
}

#[test]
fn a_partial_layer_is_noted() {
    let zip = preset(&[
        layer("static", "auto", &[(1, 1, 1)], "", 10),
        layer("breathing", "auto", &[(2, 2, 2)], "", 2),
    ]);
    let got = import(&zip, "x").unwrap();
    assert_eq!(got.setup.layers.len(), 2);
    assert_eq!(got.notes, vec!["breathing covered part of the keyboard; here it covers all of it".to_string()]);
}

#[test]
fn a_file_that_is_not_a_zip_is_refused() {
    assert_eq!(import(b"hello, not a zip", "x").unwrap_err(), PresetError::NotAZip);
}

#[test]
fn a_huge_xml_entry_is_refused() {
    let big = vec![b' '; 3 * 1024 * 1024];
    let zip = zip_of(&[("big.xml", &big)]);
    assert_eq!(import(&zip, "x").unwrap_err(), PresetError::TooLarge);
}

#[test]
fn input_over_five_megabytes_is_refused() {
    let big = vec![0u8; 5 * 1024 * 1024 + 1];
    assert_eq!(import(&big, "x").unwrap_err(), PresetError::TooLarge);
}

#[test]
fn a_zip_without_xml_is_refused() {
    let zip = zip_of(&[("readme.txt", b"hi")]);
    assert_eq!(import(&zip, "x").unwrap_err(), PresetError::NoXml);
}

#[test]
fn bad_xml_is_an_error_not_a_panic() {
    let zip = zip_of(&[("a.xml", b"<LightingEffects><EffectLayers>")]);
    assert!(matches!(import(&zip, "x").unwrap_err(), PresetError::BadXml(_)));
    let deep = format!("{}{}", "<a>".repeat(5000), "</a>".repeat(5000));
    let zip = zip_of(&[("a.xml", deep.as_bytes())]);
    assert!(matches!(import(&zip, "x").unwrap_err(), PresetError::BadXml(_)));
}

#[test]
fn name_falls_back_to_the_hint_without_extension() {
    let xml = b"<LightingEffects><EffectLayers></EffectLayers></LightingEffects>";
    let zip = zip_of(&[("a.xml", xml)]);
    assert_eq!(import(&zip, "My Cool.ChromaEffects").unwrap().setup.name, "My Cool");
}

#[test]
fn wave_without_colours_is_rainbow_and_missing_starlight_colours_are_random() {
    let zip = preset(&[layer("wave", "auto", &[], "", 0), layer("starlight", "auto", &[], "", 0)]);
    let got = import(&zip, "x").unwrap();
    assert!(matches!(&got.setup.layers[0].effect, Effect::Wave { palette: Palette::Rainbow, .. }));
    assert!(matches!(&got.setup.layers[1].effect, Effect::Starlight { palette: Palette::Random, .. }));
}
