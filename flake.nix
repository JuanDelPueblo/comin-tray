{
  description = "A small StatusNotifierItem tray for Comin";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    comin = {
      url = "github:nlewo/comin/v0.14.0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, comin }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          cominPackage = comin.packages.${system}.default;
          runtimePackages = [
            cominPackage
            pkgs.systemd
            pkgs.fontconfig
          ];
          # The log window uses winit through Iced. winit dlopens its Wayland
          # and X11 client libraries at runtime instead of linking them, so
          # they must be added to the binary's rpath explicitly.
          windowLibraries = [
            pkgs.libGL
            pkgs.vulkan-loader
            pkgs.libxkbcommon
            pkgs.wayland
            pkgs.libx11
            pkgs.libxcursor
            pkgs.libxi
            pkgs.libxcb
            pkgs.libxrandr
          ];
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "comin-tray";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.makeWrapper ];

            postInstall = ''
              install -Dm644 data/comin-tray.desktop \
                $out/share/applications/comin-tray.desktop

              patchelf --add-rpath ${nixpkgs.lib.makeLibraryPath windowLibraries} \
                $out/bin/comin-tray

              wrapProgram $out/bin/comin-tray \
                --prefix PATH : ${nixpkgs.lib.makeBinPath runtimePackages}
            '';

            # Skip the default rpath shrinking: it only keeps entries used to
            # resolve DT_NEEDED symbols, but winit's window libraries above
            # are dlopened at runtime and would otherwise be stripped back out.
            dontPatchELF = true;

            meta = {
              description = "A small StatusNotifierItem tray for Comin";
              homepage = "https://github.com/JuanDelPueblo/comin-tray";
              license = nixpkgs.lib.licenses.mit;
              mainProgram = "comin-tray";
              platforms = nixpkgs.lib.platforms.linux;
            };
          };
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/comin-tray";
        };
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.clippy
              pkgs.rustc
              pkgs.rustfmt
            ];
          };
        }
      );
    };
}
