#!/usr/bin/env nu

let niri_dir = "./niri"
let patch_file = "./niri-expose-is-fullscreen-ipc.patch"
let niri_repo = "https://github.com/YaLTeR/niri.git"

if not ($patch_file | path exists) {
    error make { msg: $"Patch file not found: ($patch_file)" }
}

if not ($"($niri_dir)/.git" | path exists) {
    print "Cloning latest Niri..."
    git clone --depth=1 $niri_repo $niri_dir
} else {
    print "Updating Niri..."

    git -C $niri_dir fetch --depth=1 origin main
    git -C $niri_dir reset --hard origin/main
    git -C $niri_dir clean -fdx
}

print "Niri revision:"
git -C $niri_dir rev-parse --short HEAD

print "Checking patch..."
git -C $niri_dir apply --check $"../($patch_file | path basename)"

print "Applying patch..."
git -C $niri_dir apply $"../($patch_file | path basename)"

print "Updating niri-ipc..."
cargo update -p niri-ipc

print "Done."
