{
  description = "ironland-copositor dev environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # smithay-drm-extras pins `libdisplay-info < 0.4.0`; nixos-unstable has 0.4.0, so
    # pull the last compatible build (0.2.0) from an older channel for that one package.
    nixpkgs-old.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, nixpkgs-old, flake-utils }:
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
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "ironland-copositor";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };
          inherit nativeBuildInputs buildInputs;
        };

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
            services.getty.autologinUser = "dev";
            security.sudo.wheelNeedsPassword = false;

            # Autologin on tty1 already gives `dev` a proper logind session
            # (which is what libseat needs), so just launch the compositor
            # straight from that login shell instead of dropping to a prompt
            # first. Not `exec`'d: if the compositor exits (Logo+Q, a crash),
            # you land back at a shell to look around instead of relaunching
            # in a loop.
            environment.loginShellInit = ''
              if [ "$(tty)" = "/dev/tty1" ] && [ "$USER" = "dev" ]; then
                ironland-copositor --tty-udev
              fi
            '';

            # Plain console only: no display manager, no host compositor of any
            # kind, so the udev backend has an uncontested seat and DRM master.
            services.xserver.enable = false;

            # The compositor is built on the host (see `packages.default`
            # above) and only the resulting binary closure lands in the VM —
            # no cargo/rustc/cc/pkg-config needed here at all.
            environment.systemPackages = [ self.packages.x86_64-linux.default ];

            # Still needed at runtime: some of these libraries (GL/EGL/Vulkan
            # loaders, mesa drivers) are dlopen'd rather than linked, so no
            # RPATH baked into the binary covers them.
            environment.variables.LD_LIBRARY_PATH =
              pkgs.lib.makeLibraryPath (perSystemBuildInputs pkgs);

            virtualisation.vmVariant = {
              virtualisation.memorySize = 4096;
              virtualisation.cores = 4;
              virtualisation.graphics = true;
              virtualisation.qemu.options = [ "-vga virtio" ];
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
