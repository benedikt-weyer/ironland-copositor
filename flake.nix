{
  description = "ironland-copositor dev environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # smithay-drm-extras pins `libdisplay-info < 0.4.0`; nixos-unstable has 0.4.0, so
    # pull the last compatible build (0.2.0) from an older channel for that one package.
    nixpkgs-old.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, nixpkgs-old, flake-utils, crane }:
    let
      perSystemBuildInputs = pkgs: with pkgs; [
        wayland
        wayland-protocols
        libxkbcommon
        libinput
        seatd
        udev
        mesa
        libglvnd
        libgbm
        libdisplay-info
        vulkan-loader
        pixman
      ];
    in
    (flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (final: prev: {
              libdisplay-info = (import nixpkgs-old { inherit system; }).libdisplay-info;
            })
          ];
        };

        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        buildInputs = perSystemBuildInputs pkgs;

        craneLib = crane.mkLib pkgs;
        commonArgs = { inherit nativeBuildInputs buildInputs; };
        # Compiles just the dependency graph (smithay and the rest) against a
        # dummy stub of our own crate. `cleanCargoSource` keeps only
        # Cargo.toml/Cargo.lock/*.rs, so this derivation's hash - and Nix's
        # cache of it - is untouched by editing our source (the dummy stub
        # never reads `resources/`, so stripping it here is fine).
        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          src = craneLib.cleanCargoSource ./.;
        });
      in
      {
        # The real build needs the full source: `resources/*` is pulled in via
        # `include_bytes!`, which `cleanCargoSource` above would strip.
        packages.default = craneLib.buildPackage (commonArgs // {
          src = craneLib.path ./.;
          inherit cargoArtifacts;
        });

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            rustfmt
            clippy
          ] ++ nativeBuildInputs ++ buildInputs;

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        };
      })) // {
      # A throwaway NixOS VM for exercising the `--tty-udev` backend: it owns
      # its (virtual) DRM/input devices directly, the way real hardware does,
      # so there is no host window manager around to steal modifier keys or
      # otherwise interfere the way testing nested under `--winit` can.
      # Run it with `scripts/run-vm`.
      nixosConfigurations.compositor-vm = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ({ pkgs, ... }: {
            system.stateVersion = "24.11";

            users.users.dev = {
              isNormalUser = true;
              extraGroups = [ "wheel" "video" "input" "render" ];
              initialPassword = "dev";
            };
            security.sudo.wheelNeedsPassword = false;

            # Populates /run/opengl-driver, which is where Mesa's GBM backend
            # (dri_gbm.so) looks for its driver by NixOS convention. Without
            # this the udev backend fails at GBM device creation.
            hardware.graphics.enable = true;

            # greetd manages the VT/session lifecycle properly (PAM session,
            # logind registration) instead of a hand-rolled getty+shell hack.
            # tuigreet is just the login prompt; on successful auth it hands
            # off to `--cmd` as the actual session command. A real script
            # file (rather than a nested-quotes command string) sidesteps
            # both greetd's config parser and TOML's own quoting corner
            # cases.
            environment.etc."ironland-launch.sh".source = pkgs.writeShellScript "ironland-launch" ''
              ironland-copositor --tty-udev
            '';

            services.greetd = {
              enable = true;
              settings.default_session = {
                command = "${pkgs.tuigreet}/bin/tuigreet --remember --remember-session --cmd /etc/ironland-launch.sh";
                user = "greeter";
              };
            };

            # Plain console only: no display manager, no host compositor of any
            # kind, so the udev backend has an uncontested seat and DRM master.
            services.xserver.enable = false;

            # The compositor is built on the host (see `packages.default`
            # above) and only the resulting binary closure lands in the VM —
            # no cargo/rustc/cc/pkg-config needed here at all.
            environment.systemPackages = [
              self.packages.x86_64-linux.default
              pkgs.alacritty
              pkgs.firefox
              pkgs.blueman
            ];

            # Still needed at runtime: some of these libraries (GL/EGL/Vulkan
            # loaders, mesa drivers) are dlopen'd rather than linked, so no
            # RPATH baked into the binary covers them.
            environment.variables.LD_LIBRARY_PATH =
              pkgs.lib.makeLibraryPath (perSystemBuildInputs pkgs);

            virtualisation.vmVariant = {
              virtualisation.memorySize = 4096;
              virtualisation.cores = 4;
              virtualisation.graphics = true;
              # Not `-vga virtio`: that's a legacy-VGA-compatible variant of
              # virtio-gpu that doesn't properly support the atomic KMS API
              # smithay requires, causing "unable to become drm master" /
              # atomic-commit EINVAL errors. A plain virtio-gpu device (no
              # VGA compatibility shim) does support atomic modesetting.
              #
              # The `-gl` variant (with a GL-enabled display) is needed on top
              # of that: plain virtio-gpu-pci exposes no DRM render node at
              # all (GBM allocation fails with NoRenderNode), only the `-gl`
              # (virgl) variant registers one. This works fine with pure
              # software rendering on the host, no real GPU passthrough
              # needed.
              virtualisation.qemu.options = [
                "-vga none"
                "-device virtio-gpu-gl-pci"
                "-display gtk,gl=on"
              ];
              # Handy for looking at source/config from inside the VM; not
              # needed to run the compositor itself, which is preinstalled.
              virtualisation.sharedDirectories.project = {
                # Set by scripts/run-vm; falls back to this repo's checkout
                # location for a manual `nix build --impure`.
                source = let v = builtins.getEnv "IRONLAND_VM_PROJECT_DIR"; in
                  if v != "" then v else "/mnt/local-storage/git/low-level/ironland-copositor";
                target = "/home/dev/project";
              };
            };
          })
        ];
      };
    };
}
