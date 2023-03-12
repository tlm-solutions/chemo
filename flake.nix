{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.11";
    utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "utils";
    };
  };

  outputs = inputs@{ self, nixpkgs, utils, crane }:
    utils.lib.eachDefaultSystem
    (system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.lib.${system};
      package = pkgs.callPackage ./derivation.nix { craneLib = craneLib; };
    in
    rec {
    checks = packages;
    packages = {
      chemo = package;
      default = package;
    };
    devShells.default = pkgs.mkShell {
      nativeBuildInputs = (with packages.chemo; nativeBuildInputs ++ buildInputs);
    };
    apps = {
      chemo = utils.lib.mkApp { drv = packages.chemo; };
      default = apps.chemo;
    };
  }) // {
    overlays.default = final: prev: {
      inherit (self.packages.${prev.system})
      chemo;
    };
  };
}
