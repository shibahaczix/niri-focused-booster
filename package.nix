{ lib
, rustPlatform
, pkg-config
, libxcb
}:

rustPlatform.buildRustPackage {
  pname = "niri-focused-booster";
  version = "0.3.0";

  src = lib.cleanSource ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    libxcb
  ];

  meta = {
    description = "Prioritize GPU memory for the focused Niri application";
    homepage = "https://github.com/1Naim/niri-focused-booster";
    license = lib.licenses.gpl3Only;
    mainProgram = "niri-focused-booster";
    platforms = lib.platforms.linux;
  };
}
