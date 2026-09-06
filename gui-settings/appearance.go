package main

import "os/exec"

// setSystemColorScheme pushes dark/light mode out to the rest of the
// desktop, not just this app's own config.toml. The compositor has no
// portal backend of its own, so this writes to the same GNOME setting
// (org.gnome.desktop.interface color-scheme) that xdg-desktop-portal-gtk
// reads from when answering an app's org.freedesktop.portal.Settings
// ("org.freedesktop.appearance", "color-scheme") query — the closest thing
// to a standard XDG mechanism for this without running a full portal
// backend ourselves. It's best-effort: a system without gsettings/GNOME's
// schema (no xdg-desktop-portal-gtk installed) just leaves this a no-op
// beyond the toggle's own config.toml entry.
func setSystemColorScheme(dark bool) error {
	value := "default"
	if dark {
		value = "prefer-dark"
	}
	return exec.Command("gsettings", "set", "org.gnome.desktop.interface", "color-scheme", value).Run()
}
