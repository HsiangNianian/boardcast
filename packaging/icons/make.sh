#!/bin/sh
# Rebuild every app icon from icon.py. Needs python3, rsvg-convert (librsvg), png2icns (libicns) and
# ImageMagick. The outputs are committed, so packaging never runs this.
#   macos.svg  -> icon.icns              macOS bundle (Big Sur grid, with shadow)
#   desktop.svg -> 128x128@2x.png         Linux hicolor 256x256
#   desktop.svg -> icon.ico               Windows, embedded in boardcast.exe by crates/ui/build.rs
#   ios.svg    -> ios-1024.png           App Store Connect, full bleed, no alpha
set -eu
cd "$(dirname "$0")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for kind in macos desktop ios; do python3 icon.py "$kind" > "$kind.svg"; done

for s in 16 32 48 128 256 512 1024; do rsvg-convert -w $s -h $s macos.svg -o "$tmp/mac-$s.png"; done
png2icns icon.icns "$tmp"/mac-16.png "$tmp"/mac-32.png "$tmp"/mac-48.png "$tmp"/mac-128.png \
  "$tmp"/mac-256.png "$tmp"/mac-512.png "$tmp"/mac-1024.png

rsvg-convert -w 256 -h 256 desktop.svg -o 128x128@2x.png

for s in 16 24 32 48 64 128 256; do rsvg-convert -w $s -h $s desktop.svg -o "$tmp/win-$s.png"; done
magick "$tmp"/win-16.png "$tmp"/win-24.png "$tmp"/win-32.png "$tmp"/win-48.png "$tmp"/win-64.png \
  "$tmp"/win-128.png "$tmp"/win-256.png icon.ico

rsvg-convert -w 1024 -h 1024 -b "#1e1e1e" ios.svg -o "$tmp/ios.png"
magick "$tmp/ios.png" -alpha off -define png:color-type=2 ios-1024.png
