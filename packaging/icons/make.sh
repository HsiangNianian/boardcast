#!/bin/sh
# Rebuild every app icon from icon.py. Needs python3, rsvg-convert (librsvg), png2icns (libicns) and
# ImageMagick. The outputs are committed, so packaging never runs this.
#   macos.svg  -> icon.icns              macOS bundle (Big Sur grid, with shadow)
#   desktop.svg -> 128x128@2x.png         Linux hicolor 256x256
#   desktop.svg -> icon.ico               Windows, embedded in boardcast.exe by crates/ui/build.rs
#   ios.svg    -> ios-1024.png           App Store Connect, full bleed, no alpha
#   favicon, desktop.svg, ios.svg -> landing/public/favicon.{svg,png}, apple-touch-icon.png   website
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

rsvg-convert -w 1024 -h 1024 -b "#f3efe6" ios.svg -o "$tmp/ios.png"
magick "$tmp/ios.png" -strip -alpha off -define png:color-type=2 ios-1024.png

# The website (crates/site-gen links these from every page): an SVG for browsers that take one, a PNG
# for those that do not, and the iOS home-screen icon, which iOS masks itself like the App Store one.
site=../../landing/public
python3 icon.py favicon > "$site/favicon.svg"
rsvg-convert -w 64 -h 64 desktop.svg -o "$site/favicon.png"
rsvg-convert -w 180 -h 180 -b "#f3efe6" ios.svg -o "$tmp/touch.png"
magick "$tmp/touch.png" -strip -alpha off "$site/apple-touch-icon.png"
