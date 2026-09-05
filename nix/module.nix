{ config, lib, pkgs, ... }:

let
  cfg = config.services.ironland-copositor;
  settingsFormat = pkgs.formats.toml { };
in
{
  options.services.ironland-copositor.settings = lib.mkOption {
    type = settingsFormat.type;
    default = { };
    description = ''
      Settings for ironland-copositor, written to
      `/etc/ironland-copositor/config.toml` and read by the compositor at
      startup (a restart is needed to pick up changes). See the
      compositor's `src/config.rs` for the full schema: keyboard layout
      under `keyboard` (passed straight through to xkbcommon), the
      terminal launched by the `run_terminal` shortcut under `terminal`,
      and key bindings under `shortcuts` — only the actions you set there
      override the built-in defaults, so a partial table is fine.
    '';
    example = lib.literalExpression ''
      {
        keyboard = {
          layout = "de";
          variant = "nodeadkeys";
        };
        terminal = "alacritty";
        shortcuts = {
          # Overrides just this one binding; every other shortcut keeps
          # its built-in default.
          toggle_launcher = [ "ctrl+space" ];
        };
      }
    '';
  };

  config.environment.etc."ironland-copositor/config.toml".source =
    settingsFormat.generate "ironland-copositor-config.toml" cfg.settings;
}
