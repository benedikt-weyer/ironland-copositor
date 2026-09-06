//! User-facing settings: keyboard layout and keyboard shortcuts.
//!
//! Settings are read from the first of these that exists, in order:
//!
//! 1. `$IRONLAND_COMPOSITOR_CONFIG` (an explicit path, mainly for testing)
//! 2. `$XDG_CONFIG_HOME/ironland-copositor/config.toml` (or `~/.config/...`)
//! 3. `/etc/ironland-copositor/config.toml` (written by the NixOS module)
//!
//! None of these existing is not an error: the compositor falls back to the
//! defaults below, which reproduce the shortcuts that used to be hardcoded.
//! A malformed file is logged and ignored rather than treated as fatal,
//! since a typo in a config file shouldn't stop the compositor from
//! starting.

use std::{collections::HashMap, env, fs, path::PathBuf};

use serde::Deserialize;
use smithay::{
    input::keyboard::{Keysym, ModifiersState, XkbConfig, xkb},
    utils::{Logical, Point, Rectangle, Size},
};
use tracing::warn;

/// Keyboard layout settings, passed straight through to xkbcommon.
///
/// An empty string for any field means "let xkbcommon fall back to its
/// `XKB_DEFAULT_*` environment variables / built-in default", matching
/// [`XkbConfig`]'s own default behavior.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct KeyboardSettings {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
}

impl KeyboardSettings {
    pub fn to_xkb_config(&self) -> XkbConfig<'_> {
        XkbConfig {
            rules: &self.rules,
            model: &self.model,
            layout: &self.layout,
            variant: &self.variant,
            options: if self.options.is_empty() {
                None
            } else {
                Some(self.options.clone())
            },
        }
    }
}

/// Which modifiers a keybinding requires. Caps lock/num lock/level3-4 shift
/// are deliberately not part of a binding's identity: only ctrl/alt/shift/
/// logo distinguish one shortcut from another here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl KeyModifiers {
    pub fn matches(&self, mods: &ModifiersState) -> bool {
        self.ctrl == mods.ctrl && self.alt == mods.alt && self.shift == mods.shift && self.logo == mods.logo
    }
}

/// Where to place an output relative to another, already-placed one, or at
/// an explicit logical position. Kept separate from [`OutputSettings::mirror_of`]:
/// mirroring takes priority over `position` when both are set for the same
/// output.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum OutputPosition {
    RightOf { right_of: String },
    LeftOf { left_of: String },
    Above { above: String },
    Below { below: String },
    Absolute { x: i32, y: i32 },
}

/// Per-output settings, keyed by connector name (e.g. `"eDP-1"`,
/// `"HDMI-A-1"`) in the `[outputs.*]` config table.
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct OutputSettings {
    /// Marks this the primary monitor. At most one output should set this;
    /// if several do, which one wins is unspecified.
    pub primary: bool,
    /// Name of another output to duplicate ("mirror"/"clone" this one onto),
    /// by placing this output at that output's position. Takes priority over
    /// `position`.
    pub mirror_of: Option<String>,
    /// Where to place this output when it isn't mirroring another one.
    /// Defaults to auto-placement (stacked to the right of every other
    /// placed output), matching pre-existing behavior.
    pub position: Option<OutputPosition>,
}

/// Resolves where a newly-connecting output named `name`, with logical size
/// `size`, should be placed, given its [`OutputSettings`] and the outputs
/// already placed in the space (`name -> current geometry`).
///
/// Falls back to auto-placement (and logs a warning) if a referenced output
/// (`mirror_of`/`right_of`/etc.) hasn't connected yet.
pub fn resolve_output_position(
    settings: &OutputSettings,
    name: &str,
    size: Size<i32, Logical>,
    placed: &[(String, Rectangle<i32, Logical>)],
) -> Point<i32, Logical> {
    let find = |target: &str| placed.iter().find(|(n, _)| n == target).map(|(_, rect)| *rect);

    if let Some(target) = settings.mirror_of.as_deref() {
        match find(target) {
            Some(rect) => return rect.loc,
            None => warn!(output = name, mirror_of = target, "Mirror target not connected yet, using auto placement"),
        }
    }

    match &settings.position {
        Some(OutputPosition::Absolute { x, y }) => return (*x, *y).into(),
        Some(OutputPosition::RightOf { right_of }) => match find(right_of) {
            Some(rect) => return (rect.loc.x + rect.size.w, rect.loc.y).into(),
            None => warn!(output = name, right_of, "Reference output not connected yet, using auto placement"),
        },
        Some(OutputPosition::LeftOf { left_of }) => match find(left_of) {
            Some(rect) => return (rect.loc.x - size.w, rect.loc.y).into(),
            None => warn!(output = name, left_of, "Reference output not connected yet, using auto placement"),
        },
        Some(OutputPosition::Above { above }) => match find(above) {
            Some(rect) => return (rect.loc.x, rect.loc.y - size.h).into(),
            None => warn!(output = name, above, "Reference output not connected yet, using auto placement"),
        },
        Some(OutputPosition::Below { below }) => match find(below) {
            Some(rect) => return (rect.loc.x, rect.loc.y + rect.size.h).into(),
            None => warn!(output = name, below, "Reference output not connected yet, using auto placement"),
        },
        None => {}
    }

    let x = placed.iter().map(|(_, rect)| rect.loc.x + rect.size.w).max().unwrap_or(0);
    (x, 0).into()
}

/// A single parsed keybinding: the modifiers/key it fires on, and the name
/// of the action to run (looked up against the compositor's own action
/// table, since the set of possible actions is compositor-specific and this
/// module only knows about parsing).
#[derive(Debug, Clone)]
pub struct Keybinding {
    pub modifiers: KeyModifiers,
    pub keysym: Keysym,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    keyboard: KeyboardSettings,
    terminal: Option<String>,
    top_bar: bool,
    shortcuts: HashMap<String, Vec<String>>,
    outputs: HashMap<String, OutputSettings>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub keyboard: KeyboardSettings,
    pub terminal: String,
    /// Whether an external top bar/shell (e.g. caelestia-shell) should be
    /// started alongside the compositor. Off by default: the compositor
    /// itself never draws one, so this only matters to launch scripts that
    /// check it. See `nix/module.nix` for how the NixOS module uses it.
    pub top_bar: bool,
    /// action name -> key combos, e.g. `"toggle_launcher" -> ["ctrl+space"]`.
    /// Always fully populated: entries not overridden by the config file
    /// keep their built-in default.
    pub shortcuts: HashMap<String, Vec<String>>,
    /// connector name (e.g. `"eDP-1"`) -> settings for that output. Outputs
    /// not present here use [`OutputSettings::default`] (auto-placed,
    /// extended, not primary).
    pub outputs: HashMap<String, OutputSettings>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            keyboard: KeyboardSettings::default(),
            terminal: default_terminal(),
            top_bar: false,
            shortcuts: default_shortcuts(),
            outputs: HashMap::new(),
        }
    }
}

fn default_terminal() -> String {
    "weston-terminal".to_string()
}

/// The shortcuts the compositor shipped with before it became configurable.
/// Kept as the baseline so an empty/missing/partial config file still
/// behaves exactly as before.
fn default_shortcuts() -> HashMap<String, Vec<String>> {
    [
        ("quit", vec!["super+alt+backspace", "super+q"]),
        ("run_terminal", vec!["super+c"]),
        ("toggle_launcher", vec!["super+space"]),
        ("toggle_floating", vec!["super+shift+space"]),
        ("kill_window", vec!["super+x"]),
        ("focus_left", vec!["super+left"]),
        ("focus_right", vec!["super+right"]),
        ("focus_up", vec!["super+up"]),
        ("focus_down", vec!["super+down"]),
        ("swap_left", vec!["super+shift+left"]),
        ("swap_right", vec!["super+shift+right"]),
        ("swap_up", vec!["super+shift+up"]),
        ("swap_down", vec!["super+shift+down"]),
        ("resize_left", vec!["super+alt+left"]),
        ("resize_right", vec!["super+alt+right"]),
        ("resize_up", vec!["super+alt+up"]),
        ("resize_down", vec!["super+alt+down"]),
        ("scale_up", vec!["super+shift+p"]),
        ("scale_down", vec!["super+shift+m"]),
        ("toggle_preview", vec!["super+shift+w"]),
        ("rotate_output", vec!["super+shift+r"]),
        ("toggle_tint", vec!["super+shift+t"]),
        ("toggle_decorations", vec!["super+shift+d"]),
    ]
    .into_iter()
    .map(|(name, keys)| (name.to_string(), keys.into_iter().map(String::from).collect()))
    .collect()
}

/// All action names the compositor understands, for validation and for the
/// settings GUI. Kept next to `default_shortcuts` so the two can't drift.
pub fn known_actions() -> Vec<&'static str> {
    [
        "quit",
        "run_terminal",
        "toggle_launcher",
        "toggle_floating",
        "kill_window",
        "focus_left",
        "focus_right",
        "focus_up",
        "focus_down",
        "swap_left",
        "swap_right",
        "swap_up",
        "swap_down",
        "resize_left",
        "resize_right",
        "resize_up",
        "resize_down",
        "scale_up",
        "scale_down",
        "toggle_preview",
        "rotate_output",
        "toggle_tint",
        "toggle_decorations",
    ]
    .to_vec()
}

fn config_search_path() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(explicit) = env::var("IRONLAND_COMPOSITOR_CONFIG") {
        paths.push(PathBuf::from(explicit));
    }

    let config_home = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Ok(config_home) = config_home {
        paths.push(config_home.join("ironland-copositor/config.toml"));
    }

    paths.push(PathBuf::from("/etc/ironland-copositor/config.toml"));

    paths
}

impl Config {
    /// Loads settings from the first config file found on
    /// [`config_search_path`], falling back to [`Config::default`] if none
    /// exist or the one found doesn't parse.
    pub fn load() -> Config {
        for path in config_search_path() {
            let contents = match fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => {
                    warn!(path = %path.display(), %err, "Failed to read compositor config, skipping it");
                    continue;
                }
            };

            let raw: RawConfig = match toml::from_str(&contents) {
                Ok(raw) => raw,
                Err(err) => {
                    warn!(path = %path.display(), %err, "Failed to parse compositor config, using defaults");
                    return Config::default();
                }
            };

            let mut shortcuts = default_shortcuts();
            shortcuts.extend(raw.shortcuts);

            tracing::info!(path = %path.display(), "Loaded compositor config");
            return Config {
                keyboard: raw.keyboard,
                terminal: raw.terminal.unwrap_or_else(default_terminal),
                top_bar: raw.top_bar,
                shortcuts,
                outputs: raw.outputs,
            };
        }

        Config::default()
    }

    /// Settings for the output named `name`, or the all-default settings
    /// (auto-placed, extended, not primary) if it isn't configured.
    pub fn output_settings(&self, name: &str) -> OutputSettings {
        self.outputs.get(name).cloned().unwrap_or_default()
    }

    /// Name of the output marked `primary = true`, if any. If more than one
    /// is marked primary, which one wins is unspecified.
    pub fn primary_output_name(&self) -> Option<&str> {
        self.outputs
            .iter()
            .find(|(_, settings)| settings.primary)
            .map(|(name, _)| name.as_str())
    }

    /// Parses every configured binding into `(modifiers, keysym, action
    /// name)` triples, skipping (with a warning) any binding that doesn't
    /// parse or whose action name isn't recognized.
    pub fn parsed_keybindings(&self) -> Vec<Keybinding> {
        let known = known_actions();
        let mut bindings = Vec::new();

        for (action, specs) in &self.shortcuts {
            if !known.contains(&action.as_str()) {
                warn!(action, "Unknown action in [shortcuts] config, ignoring");
                continue;
            }

            for spec in specs {
                match parse_binding(spec) {
                    Some((modifiers, keysym)) => bindings.push(Keybinding {
                        modifiers,
                        keysym,
                        action: action.clone(),
                    }),
                    None => warn!(action, spec, "Failed to parse keybinding, ignoring"),
                }
            }
        }

        bindings
    }
}

/// Parses a binding spec like `"ctrl+shift+left"` into its modifiers and
/// keysym. The last `+`-separated token is the key; everything before it is
/// a modifier name (`ctrl`/`control`, `alt`, `shift`, `super`/`logo`/`meta`).
fn parse_binding(spec: &str) -> Option<(KeyModifiers, Keysym)> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).filter(|s| !s.is_empty()).collect();
    let (mod_parts, key_part) = parts.split_last()?;

    let mut modifiers = KeyModifiers::default();
    for part in key_part {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "super" | "logo" | "meta" | "win" => modifiers.logo = true,
            other => {
                warn!(modifier = other, spec, "Unknown modifier in keybinding");
                return None;
            }
        }
    }

    let keysym = parse_key_name(mod_parts, modifiers.shift)?;
    Some((modifiers, keysym))
}

/// Resolves a key name to a keysym. Single ASCII letters are special-cased:
/// xkb has distinct keysyms for the lower- and upper-case forms of a letter
/// (`a` vs `A`), and it's the upper-case one that a physical key reports
/// once `modified_sym()` has applied an active Shift — so a binding that
/// asks for Shift always resolves the letter to its upper-case keysym,
/// regardless of how the user cased it in the config.
fn parse_key_name(name: &str, shift: bool) -> Option<Keysym> {
    let mut chars = name.chars();
    let keysym = match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => {
            let letter = if shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() };
            xkb::keysym_from_name(&letter.to_string(), xkb::KEYSYM_NO_FLAGS)
        }
        _ => xkb::keysym_from_name(name, xkb::KEYSYM_CASE_INSENSITIVE),
    };

    if keysym.raw() == 0 { None } else { Some(keysym) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_binding() {
        let (mods, sym) = parse_binding("ctrl+q").unwrap();
        assert_eq!(mods, KeyModifiers { ctrl: true, ..Default::default() });
        assert_eq!(sym, Keysym::q);
    }

    #[test]
    fn shift_uppercases_letter_bindings() {
        let (mods, sym) = parse_binding("ctrl+shift+m").unwrap();
        assert!(mods.shift);
        assert_eq!(sym, Keysym::M);
    }

    #[test]
    fn parses_named_keys_case_insensitively() {
        let (_, sym) = parse_binding("ctrl+RETURN").unwrap();
        assert_eq!(sym, Keysym::Return);

        let (_, sym) = parse_binding("ctrl+alt+backspace").unwrap();
        assert_eq!(sym, Keysym::BackSpace);
    }

    #[test]
    fn rejects_unknown_modifier() {
        assert!(parse_binding("hyper+q").is_none());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_binding("ctrl+notarealkey").is_none());
    }

    #[test]
    fn every_default_shortcut_parses() {
        for (action, specs) in default_shortcuts() {
            for spec in specs {
                assert!(parse_binding(&spec).is_some(), "default binding {action}={spec} failed to parse");
            }
        }
    }

    #[test]
    fn partial_shortcuts_override_only_the_named_action() {
        let mut shortcuts = HashMap::new();
        shortcuts.insert("quit".to_string(), vec!["ctrl+alt+q".to_string()]);
        let raw = RawConfig {
            keyboard: KeyboardSettings::default(),
            terminal: None,
            top_bar: false,
            shortcuts,
            outputs: HashMap::new(),
        };

        let mut merged = default_shortcuts();
        merged.extend(raw.shortcuts);

        assert_eq!(merged["quit"], vec!["ctrl+alt+q"]);
        assert_eq!(merged["toggle_launcher"], vec!["super+space"]);
    }

    fn size(w: i32, h: i32) -> Size<i32, Logical> {
        (w, h).into()
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn top_bar_defaults_to_disabled_but_can_be_enabled() {
        let raw: RawConfig = toml::from_str("").unwrap();
        assert!(!raw.top_bar);

        let raw: RawConfig = toml::from_str("top_bar = true").unwrap();
        assert!(raw.top_bar);
    }

    #[test]
    fn parses_relative_and_absolute_positions() {
        let toml = r#"
            [outputs."HDMI-A-1"]
            position = { right_of = "eDP-1" }

            [outputs."DP-1"]
            position = { x = 100, y = 200 }

            [outputs."eDP-1"]
            primary = true
        "#;
        let raw: RawConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            raw.outputs["HDMI-A-1"].position,
            Some(OutputPosition::RightOf { right_of: "eDP-1".to_string() })
        );
        assert_eq!(raw.outputs["DP-1"].position, Some(OutputPosition::Absolute { x: 100, y: 200 }));
        assert!(raw.outputs["eDP-1"].primary);
    }

    #[test]
    fn auto_placement_stacks_to_the_right() {
        let settings = OutputSettings::default();
        let placed = [("eDP-1".to_string(), rect(0, 0, 1920, 1080))];
        let pos = resolve_output_position(&settings, "HDMI-A-1", size(1920, 1080), &placed);
        assert_eq!(pos, (1920, 0).into());
    }

    #[test]
    fn relative_positions_place_next_to_target() {
        let placed = [("eDP-1".to_string(), rect(0, 0, 1920, 1080))];

        let right = OutputSettings {
            position: Some(OutputPosition::RightOf { right_of: "eDP-1".to_string() }),
            ..Default::default()
        };
        assert_eq!(resolve_output_position(&right, "b", size(800, 600), &placed), (1920, 0).into());

        let left = OutputSettings {
            position: Some(OutputPosition::LeftOf { left_of: "eDP-1".to_string() }),
            ..Default::default()
        };
        assert_eq!(resolve_output_position(&left, "b", size(800, 600), &placed), (-800, 0).into());

        let above = OutputSettings {
            position: Some(OutputPosition::Above { above: "eDP-1".to_string() }),
            ..Default::default()
        };
        assert_eq!(resolve_output_position(&above, "b", size(800, 600), &placed), (0, -600).into());

        let below = OutputSettings {
            position: Some(OutputPosition::Below { below: "eDP-1".to_string() }),
            ..Default::default()
        };
        assert_eq!(resolve_output_position(&below, "b", size(800, 600), &placed), (0, 1080).into());
    }

    #[test]
    fn mirror_of_takes_priority_over_position() {
        let placed = [("eDP-1".to_string(), rect(0, 0, 1920, 1080))];
        let settings = OutputSettings {
            mirror_of: Some("eDP-1".to_string()),
            position: Some(OutputPosition::RightOf { right_of: "eDP-1".to_string() }),
            ..Default::default()
        };
        assert_eq!(resolve_output_position(&settings, "HDMI-A-1", size(1920, 1080), &placed), (0, 0).into());
    }

    #[test]
    fn missing_reference_output_falls_back_to_auto_placement() {
        let placed = [("eDP-1".to_string(), rect(0, 0, 1920, 1080))];
        let settings = OutputSettings {
            position: Some(OutputPosition::RightOf { right_of: "not-connected".to_string() }),
            ..Default::default()
        };
        assert_eq!(resolve_output_position(&settings, "b", size(800, 600), &placed), (1920, 0).into());
    }
}
