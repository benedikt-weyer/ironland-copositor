// Command gui-settings is a small Fyne GUI for ironland-copositor's TOML
// settings file (see src/config.rs in the main crate for the schema this
// mirrors). It edits the user's own config file at
// $XDG_CONFIG_HOME/ironland-copositor/config.toml (falling back to
// ~/.config/...), which the compositor reads at startup ahead of the
// system-wide file a NixOS module may have written to /etc. Changes need a
// compositor restart to take effect.
package main

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
)

func main() {
	a := app.NewWithID("dev.ironland.copositor-settings")
	w := a.NewWindow("ironland-copositor Settings")

	cfg, loadedFrom := loadConfig()

	keyboardTab := buildKeyboardTab(&cfg)
	shortcutsTab := buildShortcutsTab(&cfg)
	outputsTab := buildOutputsTab(&cfg, w)

	status := widget.NewLabel(statusText(loadedFrom))
	status.Wrapping = fyne.TextWrapWord

	saveButton := widget.NewButtonWithIcon("Save", nil, func() {
		path, err := saveConfig(cfg)
		if err != nil {
			dialog.ShowError(fmt.Errorf("saving settings: %w", err), w)
			return
		}
		status.SetText(fmt.Sprintf("Saved to %s. Restart the compositor to apply.", path))
	})
	saveButton.Importance = widget.HighImportance

	tabs := container.NewAppTabs(
		container.NewTabItem("Keyboard", keyboardTab),
		container.NewTabItem("Shortcuts", shortcutsTab),
		container.NewTabItem("Monitors", outputsTab),
	)

	content := container.NewBorder(nil, container.NewVBox(status, saveButton), nil, nil, tabs)
	w.SetContent(content)
	w.Resize(fyne.NewSize(640, 520))
	w.ShowAndRun()
}

func statusText(loadedFrom string) string {
	if loadedFrom == "" {
		return "No config file found yet; showing built-in defaults."
	}
	return fmt.Sprintf("Loaded from %s", loadedFrom)
}

// buildKeyboardTab lays out entries for the keyboard layout fields and the
// terminal command, writing every edit straight back into cfg so Save
// always has the current values.
func buildKeyboardTab(cfg *Config) fyne.CanvasObject {
	layout := widget.NewEntry()
	layout.SetText(cfg.Keyboard.Layout)
	layout.SetPlaceHolder("e.g. us, de, gb (empty = system default)")
	layout.OnChanged = func(s string) { cfg.Keyboard.Layout = s }

	variant := widget.NewEntry()
	variant.SetText(cfg.Keyboard.Variant)
	variant.SetPlaceHolder("e.g. nodeadkeys, dvorak (optional)")
	variant.OnChanged = func(s string) { cfg.Keyboard.Variant = s }

	model := widget.NewEntry()
	model.SetText(cfg.Keyboard.Model)
	model.SetPlaceHolder("e.g. pc105 (optional)")
	model.OnChanged = func(s string) { cfg.Keyboard.Model = s }

	options := widget.NewEntry()
	options.SetText(cfg.Keyboard.Options)
	options.SetPlaceHolder("e.g. caps:swapescape (optional)")
	options.OnChanged = func(s string) { cfg.Keyboard.Options = s }

	rules := widget.NewEntry()
	rules.SetText(cfg.Keyboard.Rules)
	rules.SetPlaceHolder("advanced: xkb rules file (usually leave empty)")
	rules.OnChanged = func(s string) { cfg.Keyboard.Rules = s }

	terminal := widget.NewEntry()
	terminal.SetText(cfg.Terminal)
	terminal.SetPlaceHolder("command run by the \"Open terminal\" shortcut")
	terminal.OnChanged = func(s string) { cfg.Terminal = s }

	form := widget.NewForm(
		widget.NewFormItem("Layout", layout),
		widget.NewFormItem("Variant", variant),
		widget.NewFormItem("Model", model),
		widget.NewFormItem("Options", options),
		widget.NewFormItem("Rules", rules),
		widget.NewFormItem("Terminal command", terminal),
	)

	hint := widget.NewLabel("Keyboard fields are passed straight to xkbcommon; leave a field empty to use its system default.")
	hint.Wrapping = fyne.TextWrapWord

	return container.NewVBox(form, hint)
}

// buildShortcutsTab lays out one editable row per known action, each
// holding its bound key combos as a comma-separated list (e.g.
// "ctrl+left, ctrl+kp_left").
func buildShortcutsTab(cfg *Config) fyne.CanvasObject {
	form := widget.NewForm()

	for _, action := range knownActions {
		action := action // capture for the closure below
		entry := widget.NewEntry()
		entry.SetText(strings.Join(cfg.Shortcuts[action], ", "))
		entry.SetPlaceHolder("e.g. ctrl+shift+q")
		entry.OnChanged = func(s string) {
			cfg.Shortcuts[action] = splitKeyCombos(s)
		}

		label := actionLabels[action]
		if label == "" {
			label = action
		}
		form.Append(label, entry)
	}

	hint := widget.NewLabel("Separate multiple key combos for the same action with commas. A combo is modifiers and a key joined with '+', e.g. \"ctrl+alt+left\".")
	hint.Wrapping = fyne.TextWrapWord

	return container.NewBorder(nil, hint, nil, nil, container.NewVScroll(form))
}

func splitKeyCombos(s string) []string {
	var combos []string
	for _, part := range strings.Split(s, ",") {
		part = strings.TrimSpace(part)
		if part != "" {
			combos = append(combos, part)
		}
	}
	return combos
}
