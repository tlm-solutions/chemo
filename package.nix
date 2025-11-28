{ lib, rustPlatform, pkg-config, openssl, protobuf, libpq, ... }:

let
  manifest = (lib.importTOML ./Cargo.toml).package;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = manifest.name;
  inherit (manifest) version;

  src = lib.cleanSource ./.;

  cargoHash = "sha256-nvWpk4p/ANiJNmUh8NcO0UgEIGjysLc9oI0b9OgL260=";

  cargoBuildFlags = "-p ${finalAttrs.pname}";
  cargoTestFlags = "-p ${finalAttrs.pname}";

  nativeBuildInputs = [ pkg-config protobuf ];

  buildInputs = [ openssl libpq ];

  meta = {
    mainProgram = "chemo";
    description = "Merging high end quality TM Data Streams since 2023";
    homepage = "https://github.com/dump-dvb/chemo";
  };
})

