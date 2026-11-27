//! Reads the default profile of macOS Terminal.app (font and colours), so the built-in terminal
//! looks like the one the user already has set up.
//!
//! Profiles live in `~/Library/Preferences/com.apple.Terminal.plist`; fonts and colours inside
//! are `NSKeyedArchiver` blobs. Anything missing or unreadable is left as `None` for the GUI to
//! fill with its own defaults.

use std::path::Path;

use plist::{Dictionary, Value};
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalProfile {
    pub name: String,
    /// PostScript name, e.g. `SFMonoTerminal-Regular`.
    pub font: Option<String>,
    /// Points; equal to CSS pixels on macOS.
    pub font_size: Option<f64>,
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub bold: Option<String>,
    pub cursor: Option<String>,
    pub selection: Option<String>,
    /// black, red, green, yellow, blue, magenta, cyan, white, then the bright variants.
    pub ansi: Vec<Option<String>>,
}

const ANSI_KEYS: [&str; 16] = [
    "ANSIBlackColor",
    "ANSIRedColor",
    "ANSIGreenColor",
    "ANSIYellowColor",
    "ANSIBlueColor",
    "ANSIMagentaColor",
    "ANSICyanColor",
    "ANSIWhiteColor",
    "ANSIBrightBlackColor",
    "ANSIBrightRedColor",
    "ANSIBrightGreenColor",
    "ANSIBrightYellowColor",
    "ANSIBrightBlueColor",
    "ANSIBrightMagentaColor",
    "ANSIBrightCyanColor",
    "ANSIBrightWhiteColor",
];

/// The profile Terminal.app opens new windows with, if Terminal.app has preferences.
pub fn default_profile() -> Option<TerminalProfile> {
    let home = std::env::var_os("HOME")?;
    read(&Path::new(&home).join("Library/Preferences/com.apple.Terminal.plist"))
}

fn read(path: &Path) -> Option<TerminalProfile> {
    let prefs = Value::from_file(path).ok()?.into_dictionary()?;
    let name = prefs
        .get("Default Window Settings")?
        .as_string()?
        .to_owned();
    let profile = prefs
        .get("Window Settings")?
        .as_dictionary()?
        .get(&name)?
        .as_dictionary()?;
    Some(parse(name, profile))
}

fn parse(name: String, profile: &Dictionary) -> TerminalProfile {
    let color = |key: &str| archived(profile, key).and_then(|o| color_of(&o));
    let font = archived(profile, "Font");
    TerminalProfile {
        name,
        font: font.as_ref().and_then(font_name),
        font_size: font
            .as_ref()
            .and_then(|o| o.root.get("NSSize"))
            .and_then(Value::as_real),
        background: color("BackgroundColor"),
        foreground: color("TextColor"),
        bold: color("TextBoldColor"),
        cursor: color("CursorColor"),
        selection: color("SelectionColor"),
        ansi: ANSI_KEYS.iter().map(|key| color(key)).collect(),
    }
}

/// The root object of an `NSKeyedArchiver` blob, with the object table to resolve references.
struct Archived {
    objects: Vec<Value>,
    root: Dictionary,
}

fn archived(profile: &Dictionary, key: &str) -> Option<Archived> {
    let data = profile.get(key)?.as_data()?;
    let archive = Value::from_reader(std::io::Cursor::new(data))
        .ok()?
        .into_dictionary()?;
    let objects = archive.get("$objects")?.as_array()?.clone();
    let root_ref = archive
        .get("$top")?
        .as_dictionary()?
        .get("root")?
        .as_uid()?
        .get();
    let root = objects
        .get(usize::try_from(root_ref).ok()?)?
        .as_dictionary()?
        .clone();
    Some(Archived { objects, root })
}

fn font_name(font: &Archived) -> Option<String> {
    let index = font.root.get("NSName")?.as_uid()?.get();
    Some(
        font.objects
            .get(usize::try_from(index).ok()?)?
            .as_string()?
            .to_owned(),
    )
}

/// `NSColor` as `#rrggbb`. Handles RGB (`NSRGB` = "r g b [a]") and grayscale (`NSWhite`).
fn color_of(color: &Archived) -> Option<String> {
    let components = |key: &str| -> Option<Vec<f64>> {
        let bytes = color.root.get(key)?.as_data()?;
        let text = std::str::from_utf8(bytes).ok()?.trim_end_matches('\0');
        text.split_whitespace().map(|n| n.parse().ok()).collect()
    };
    let [r, g, b] = if let Some(rgb) = components("NSRGB") {
        [*rgb.first()?, *rgb.get(1)?, *rgb.get(2)?]
    } else {
        let white = *components("NSWhite")?.first()?;
        [white; 3]
    };
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::Uid;

    /// Builds an `NSKeyedArchiver` blob whose root is `root`, with `extra` objects after it.
    fn archive(root: Dictionary, extra: Vec<Value>) -> Value {
        let mut objects = vec![Value::String("$null".into()), Value::Dictionary(root)];
        objects.extend(extra);
        let mut top = Dictionary::new();
        top.insert("root".into(), Value::Uid(Uid::new(1)));
        let mut archive = Dictionary::new();
        archive.insert("$top".into(), Value::Dictionary(top));
        archive.insert("$objects".into(), Value::Array(objects));
        let mut bytes = Vec::new();
        Value::Dictionary(archive)
            .to_writer_binary(&mut bytes)
            .unwrap();
        Value::Data(bytes)
    }

    fn rgb(text: &str) -> Value {
        let mut color = Dictionary::new();
        color.insert(
            "NSRGB".into(),
            Value::Data(format!("{text}\0").into_bytes()),
        );
        archive(color, vec![])
    }

    #[test]
    fn reads_font_and_colours() {
        let mut font = Dictionary::new();
        font.insert("NSName".into(), Value::Uid(Uid::new(2)));
        font.insert("NSSize".into(), Value::Real(12.0));
        let mut white = Dictionary::new();
        white.insert("NSWhite".into(), Value::Data(b"0.5 1\0".to_vec()));

        let mut profile = Dictionary::new();
        profile.insert(
            "Font".into(),
            archive(font, vec!["SFMonoTerminal-Regular".into()]),
        );
        profile.insert("BackgroundColor".into(), rgb("0.0983 0.1139 0.1526 0.95"));
        profile.insert("TextColor".into(), archive(white, vec![]));
        profile.insert("ANSIRedColor".into(), rgb("1 0 0"));

        let p = parse("Clear Dark".into(), &profile);
        assert_eq!(p.font.as_deref(), Some("SFMonoTerminal-Regular"));
        assert_eq!(p.font_size, Some(12.0));
        assert_eq!(p.background.as_deref(), Some("#191d27"));
        assert_eq!(p.foreground.as_deref(), Some("#808080"));
        assert_eq!(p.ansi.len(), 16);
        assert_eq!(p.ansi[1].as_deref(), Some("#ff0000"));
        assert_eq!(p.ansi[0], None);
        assert_eq!(p.cursor, None);
    }

    #[test]
    fn reads_this_machine_if_present() {
        // Smoke test against the real preferences; skipped where Terminal.app was never used.
        if let Some(p) = default_profile() {
            println!("{p:?}");
            assert_eq!(p.ansi.len(), 16);
        }
    }
}
