{ craneLib, src, lib, cmake, pkg-config, protobuf, grpc, openssl, postgresql_14}:

craneLib.buildPackage {
  pname = "chemo";
  version = "0.1.0";

  src = ./.;

  buildInputs = [ cmake protobuf grpc openssl postgresql_14];
  nativeBuildInputs = [ pkg-config ];

  meta = {
    description = "Merging high end quality TM Data Streams since 2023";
    homepage = "https://github.com/dump-dvb/chemo";
  };
}
