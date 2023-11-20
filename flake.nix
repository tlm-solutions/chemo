{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";
    utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "utils";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ self, nixpkgs, utils, crane, fenix}:
    utils.lib.eachDefaultSystem
    (system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.lib.${system}.overrideToolchain fenix.packages.${system}.minimal.toolchain;
      package = pkgs.callPackage ./derivation.nix { craneLib = craneLib; };
    in
    rec {
    checks = packages;
    packages = {
      chemo = package;
      default = package;
      docs = (pkgs.nixosOptionsDoc {
        options = (nixpkgs.lib.nixosSystem {
          inherit system;
          modules = [ self.nixosModules.default ];
        }).options.TLMS;
      }).optionsCommonMark;
    };
    devShells.default = pkgs.mkShell {
      nativeBuildInputs = (with packages.chemo; nativeBuildInputs ++ buildInputs);
    };

    apps = {
      chemo = utils.lib.mkApp { drv = packages.chemo; };
      default = apps.chemo;
    };
  }) // {
    nixosModules = rec {
        default = chemo;
        chemo = import ./nixos-module;
    };

    overlays.default = final: prev: {
      inherit (self.packages.${prev.system})
      chemo;
    };
  };
}
