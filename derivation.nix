{ craneLib, src, lib, cmake, pkg-config, protobuf, grpc}:

craneLib.buildPackage {
  pname = "chemo";
  version = "0.1.0";

  src = ./.;

  buildInputs = [ cmake protobuf grpc ];
  nativeBuildInputs = [ pkg-config ];

  meta = {
    description = "Merging high end quality TM Data Streams since 2023";
    homepage = "https://github.com/dump-dvb/chemo";
  };
}
