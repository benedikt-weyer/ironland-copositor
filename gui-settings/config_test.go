package main

import (
	"path/filepath"
	"reflect"
	"testing"
)

func TestSplitKeyCombos(t *testing.T) {
	got := splitKeyCombos(" ctrl+q, ctrl+alt+backspace ,, ")
	want := []string{"ctrl+q", "ctrl+alt+backspace"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("splitKeyCombos: got %v, want %v", got, want)
	}
}

func TestSaveThenLoadRoundTrips(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("IRONLAND_COMPOSITOR_CONFIG", "")

	cfg := defaultConfig()
	cfg.Keyboard.Layout = "de"
	cfg.Keyboard.Variant = "nodeadkeys"
	cfg.Terminal = "alacritty"
	cfg.Shortcuts["quit"] = []string{"ctrl+alt+q"}

	path, err := saveConfig(cfg)
	if err != nil {
		t.Fatalf("saveConfig: %v", err)
	}
	if want := filepath.Join(dir, "ironland-copositor", "config.toml"); path != want {
		t.Fatalf("saveConfig path = %q, want %q", path, want)
	}

	loaded, loadedFrom := loadConfig()
	if loadedFrom != path {
		t.Fatalf("loadConfig loadedFrom = %q, want %q", loadedFrom, path)
	}
	if loaded.Keyboard.Layout != "de" || loaded.Keyboard.Variant != "nodeadkeys" {
		t.Fatalf("loadConfig keyboard = %+v", loaded.Keyboard)
	}
	if loaded.Terminal != "alacritty" {
		t.Fatalf("loadConfig terminal = %q, want alacritty", loaded.Terminal)
	}
	if !reflect.DeepEqual(loaded.Shortcuts["quit"], []string{"ctrl+alt+q"}) {
		t.Fatalf("loadConfig shortcuts[quit] = %v", loaded.Shortcuts["quit"])
	}
	// An action untouched by the override should keep its built-in default.
	if !reflect.DeepEqual(loaded.Shortcuts["toggle_launcher"], []string{"ctrl+space"}) {
		t.Fatalf("loadConfig shortcuts[toggle_launcher] = %v", loaded.Shortcuts["toggle_launcher"])
	}
}

func TestLoadConfigWithNoFileReturnsDefaults(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("IRONLAND_COMPOSITOR_CONFIG", "")

	cfg, loadedFrom := loadConfig()
	if loadedFrom != "" {
		t.Fatalf("loadedFrom = %q, want empty", loadedFrom)
	}
	if !reflect.DeepEqual(cfg, defaultConfig()) {
		t.Fatalf("loadConfig without a file = %+v, want defaults", cfg)
	}
}
