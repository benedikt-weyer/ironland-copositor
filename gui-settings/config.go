package main

import (
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

// KeyboardSettings mirrors `config::KeyboardSettings` in the compositor
// (src/config.rs): fields are passed straight through to xkbcommon, and an
// empty string means "let xkbcommon fall back to its XKB_DEFAULT_* env vars
// / built-in default".
type KeyboardSettings struct {
	Rules   string `toml:"rules"`
	Model   string `toml:"model"`
	Layout  string `toml:"layout"`
	Variant string `toml:"variant"`
	Options string `toml:"options"`
}

// Config mirrors `config::Config` / `config::RawConfig` in the compositor.
type Config struct {
	Keyboard  KeyboardSettings          `toml:"keyboard"`
	Terminal  string                    `toml:"terminal"`
	Shortcuts map[string][]string       `toml:"shortcuts"`
	Outputs   map[string]OutputSettings `toml:"outputs"`
}

// OutputPosition mirrors `config::OutputPosition` in the compositor: exactly
// one of RightOf/LeftOf/Above/Below (each another output's connector name)
// or X/Y (an absolute logical position) should be set. It's kept flat
// rather than as a Go-level tagged union because that's how it round-trips
// through TOML into Rust's `#[serde(untagged)]` enum: only the keys present
// in the table matter, and `omitempty` keeps the others out of the file.
type OutputPosition struct {
	RightOf string `toml:"right_of,omitempty"`
	LeftOf  string `toml:"left_of,omitempty"`
	Above   string `toml:"above,omitempty"`
	Below   string `toml:"below,omitempty"`
	X       *int   `toml:"x,omitempty"`
	Y       *int   `toml:"y,omitempty"`
}

// OutputSettings mirrors `config::OutputSettings` in the compositor, keyed
// by connector name (e.g. "eDP-1", "HDMI-A-1") in Config.Outputs.
type OutputSettings struct {
	Primary  bool            `toml:"primary,omitempty"`
	MirrorOf string          `toml:"mirror_of,omitempty"`
	Position *OutputPosition `toml:"position,omitempty"`
}

// knownActions lists every action the compositor recognizes in
// [shortcuts], in the same order as `config::known_actions` in
// src/config.rs. Keep the two in sync: an action missing here just won't
// be editable in the GUI, and one missing there is silently ignored by the
// compositor with a warning.
var knownActions = []string{
	"quit",
	"run_terminal",
	"toggle_launcher",
	"toggle_floating",
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
}

// actionLabels gives each action a human-readable name for the GUI.
var actionLabels = map[string]string{
	"quit":               "Quit compositor",
	"run_terminal":       "Open terminal",
	"toggle_launcher":    "Toggle app launcher",
	"toggle_floating":    "Toggle floating/tiled",
	"focus_left":         "Focus window: left",
	"focus_right":        "Focus window: right",
	"focus_up":           "Focus window: up",
	"focus_down":         "Focus window: down",
	"swap_left":          "Swap window: left",
	"swap_right":         "Swap window: right",
	"swap_up":            "Swap window: up",
	"swap_down":          "Swap window: down",
	"resize_left":        "Resize tiled window: left",
	"resize_right":       "Resize tiled window: right",
	"resize_up":          "Resize tiled window: up",
	"resize_down":        "Resize tiled window: down",
	"scale_up":           "Increase output scale",
	"scale_down":         "Decrease output scale",
	"toggle_preview":     "Toggle window preview",
	"rotate_output":      "Rotate output",
	"toggle_tint":        "Toggle debug tint",
	"toggle_decorations": "Toggle window decorations",
}

// defaultShortcuts is the baseline the compositor falls back to for any
// action not overridden in the config file. Mirrors
// `config::default_shortcuts` in src/config.rs.
func defaultShortcuts() map[string][]string {
	return map[string][]string{
		"quit":               {"super+alt+backspace", "super+q"},
		"run_terminal":       {"super+return"},
		"toggle_launcher":    {"super+space"},
		"toggle_floating":    {"super+shift+space"},
		"focus_left":         {"super+left"},
		"focus_right":        {"super+right"},
		"focus_up":           {"super+up"},
		"focus_down":         {"super+down"},
		"swap_left":          {"super+shift+left"},
		"swap_right":         {"super+shift+right"},
		"swap_up":            {"super+shift+up"},
		"swap_down":          {"super+shift+down"},
		"resize_left":        {"super+alt+left"},
		"resize_right":       {"super+alt+right"},
		"resize_up":          {"super+alt+up"},
		"resize_down":        {"super+alt+down"},
		"scale_up":           {"super+shift+p"},
		"scale_down":         {"super+shift+m"},
		"toggle_preview":     {"super+shift+w"},
		"rotate_output":      {"super+shift+r"},
		"toggle_tint":        {"super+shift+t"},
		"toggle_decorations": {"super+shift+d"},
	}
}

func defaultConfig() Config {
	return Config{
		Terminal:  "weston-terminal",
		Shortcuts: defaultShortcuts(),
		Outputs:   map[string]OutputSettings{},
	}
}

// userConfigPath is where this GUI saves settings: the same
// `$XDG_CONFIG_HOME`/`~/.config` location the compositor checks before
// falling back to `/etc/ironland-copositor/config.toml`, and one a normal
// user can write without root.
func userConfigPath() (string, error) {
	configHome := os.Getenv("XDG_CONFIG_HOME")
	if configHome == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		configHome = filepath.Join(home, ".config")
	}
	return filepath.Join(configHome, "ironland-copositor", "config.toml"), nil
}

// configSearchPath mirrors `config::config_search_path` in src/config.rs:
// the same explicit-override env var, the same user config path, then the
// system-wide file the NixOS module writes.
func configSearchPath() []string {
	var paths []string
	if explicit := os.Getenv("IRONLAND_COMPOSITOR_CONFIG"); explicit != "" {
		paths = append(paths, explicit)
	}
	if userPath, err := userConfigPath(); err == nil {
		paths = append(paths, userPath)
	}
	paths = append(paths, "/etc/ironland-copositor/config.toml")
	return paths
}

// loadConfig returns the settings that would be active if the compositor
// started right now (the first config file found on configSearchPath,
// merged over the built-in defaults), plus the path it came from - or ""
// if none of the candidate files exist yet.
func loadConfig() (Config, string) {
	cfg := defaultConfig()

	for _, path := range configSearchPath() {
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}

		var raw Config
		if _, err := toml.Decode(string(data), &raw); err != nil {
			// Malformed file: same as the compositor, fall back to defaults
			// rather than erroring out.
			return defaultConfig(), ""
		}

		if raw.Terminal != "" {
			cfg.Terminal = raw.Terminal
		}
		cfg.Keyboard = raw.Keyboard
		for action, keys := range raw.Shortcuts {
			cfg.Shortcuts[action] = keys
		}
		if raw.Outputs != nil {
			cfg.Outputs = raw.Outputs
		}
		return cfg, path
	}

	return cfg, ""
}

// saveConfig writes cfg to the user's config file, creating its parent
// directory if needed, and returns the path it wrote to.
func saveConfig(cfg Config) (string, error) {
	path, err := userConfigPath()
	if err != nil {
		return "", err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return "", err
	}

	f, err := os.Create(path)
	if err != nil {
		return "", err
	}
	defer f.Close()

	if err := toml.NewEncoder(f).Encode(cfg); err != nil {
		return "", err
	}
	return path, nil
}
