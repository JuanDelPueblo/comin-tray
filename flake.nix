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
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "comin-tray";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.makeWrapper ];

            postInstall = ''
              wrapProgram $out/bin/comin-tray \
                --prefix PATH : ${nixpkgs.lib.makeBinPath [ cominPackage ]}
            '';

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
