{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = (import nixpkgs) {
            inherit system;
          };
          chemo = pkgs.callPackage ./package.nix { };
        in
        {
          packages = {
            inherit chemo;
            default = chemo;
            docs = (pkgs.nixosOptionsDoc {
              options = (nixpkgs.lib.nixosSystem {
                inherit system;
                modules = [ self.nixosModules.default ];
              }).options.TLMS;
            }).optionsCommonMark;
          };

          devShells.default = pkgs.mkShell {
            nativeBuildInputs = with chemo; nativeBuildInputs ++ buildInputs;
          };
        }
      ) // {
      overlays.default = _: prev: {
        inherit (self.packages."${prev.system}") chemo;
      };

      nixosModules = rec {
        chemo = ./module.nix;
        default = chemo;
      };
    };
}
