{ lib, rustPlatform, pkg-config, cmake, openssl, libpq, ... }:

let
  manifest = (lib.importTOML ./Cargo.toml).package;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = manifest.name;
  inherit (manifest) version;

  src = lib.cleanSource ./.;

  cargoHash = "sha256-JCeuelqP5vCNd0A7dRnW+M3+FKUM2sUFFCyArm8VwN4=";

  cargoBuildFlags = "-p ${finalAttrs.pname}";
  cargoTestFlags = "-p ${finalAttrs.pname}";

  nativeBuildInputs = [ pkg-config cmake ];

  buildInputs = [ openssl libpq ];

  meta = {
    mainProgram = "chemo";
    description = "Merging high end quality TM Data Streams since 2023";
    homepage = "https://github.com/dump-dvb/chemo";
  };
})

