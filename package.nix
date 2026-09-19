{ lib
, rustPlatform
, pkg-config
, libxcb
, fetchFromGitHub
}:

let
  niri = fetchFromGitHub {
    owner = "YaLTeR";
    repo = "niri";
    rev = "62c230a662ac913a54fc49c86fda0406da3f3ba9";
    hash = "sha256-HDn3PxxIuHSxkC+BRna4x7WxE4nhD58qNCYP6sT9ryo=";
  };

in rustPlatform.buildRustPackage {
  pname = "niri-focused-booster";
  version = "0.3.0";

  src = builtins.path {
    path = ./.;
    name = "niri-focused-booster-source";
  };

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    libxcb
  ];

  preBuild = ''
    cp -r ${niri} ./niri
    chmod -R u+w ./niri

    cd ./niri
    patch -p1 < ../niri-expose-is-fullscreen-ipc.patch
    cd ..
  '';

  meta = {
    description = "Prioritize GPU memory for the focused Niri application";
    homepage = "https://github.com/1Naim/niri-focused-booster";
    license = lib.licenses.gpl3Only;
    mainProgram = "niri-focused-booster";
    platforms = lib.platforms.linux;
  };
}
