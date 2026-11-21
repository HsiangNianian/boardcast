# Signing and notarizing the Mac app, sourced by package.sh. Plain sh has no local variables, so the
# functions' own carry a prefix and never touch the caller's.
#
# With a Developer ID certificate in MACOS_CERT_P12 (base64) and MACOS_CERT_PASSWORD, every program and
# library in the app is signed with it under the hardened runtime, and the app goes out in a disk image
# as well as a zip: drag Alcyo to Applications and it opens, with no Control-click. With NOTARIZE=1 and an
# App Store Connect API key (APPLE_API_KEY_P8 base64, APPLE_API_KEY_ID, APPLE_API_ISSUER_ID), the disk
# image is sent to Apple's notary service and its ticket stapled to it; the app inside is covered by the
# same ticket, which Gatekeeper finds online once it is copied out. Without a certificate the app is
# signed ad hoc as before, which Apple silicon requires to run at all.

entitlements="$root/packaging/macos/entitlements.plist"
identity="-"

# Put the certificate in a keychain of its own for this run.
setup_signing() {
  [ -n "${MACOS_CERT_P12:-}" ] || { echo "no MACOS_CERT_P12: signing ad hoc" >&2; return 0; }
  work=$(mktemp -d)
  keychain="$work/alcyo-signing.keychain-db"
  password=$(openssl rand -hex 16)
  printf '%s' "$MACOS_CERT_P12" | base64 --decode > "$work/cert.p12"
  security create-keychain -p "$password" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$password" "$keychain"
  security import "$work/cert.p12" -k "$keychain" -P "$MACOS_CERT_PASSWORD" -T /usr/bin/codesign
  # Apple's Developer ID intermediate, so the chain is whole whatever the machine carries.
  curl -fsSL -o "$work/DeveloperIDG2CA.cer" https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer
  security import "$work/DeveloperIDG2CA.cer" -k "$keychain" || true
  security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" > /dev/null
  security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
  rm -f "$work/cert.p12"
  identity=$(security find-identity -v -p codesigning "$keychain" | awk '/Developer ID Application/ { print $2; exit }')
  [ -n "$identity" ] || { echo "the certificate holds no Developer ID Application identity" >&2; exit 1; }
  echo "signing with $(security find-identity -v -p codesigning "$keychain" | awk -F'"' '/Developer ID Application/ { print $2; exit }')"
}

signed() { [ "$identity" != "-" ]; }

# Every Mach-O file inside first, deepest first, then the app: codesign --deep would sign them without the
# hardened runtime or a timestamp, which notarization refuses.
sign_app() {
  sign_target=$1
  if ! signed; then
    codesign --force --deep --sign - "$sign_target"
    return
  fi
  find "$sign_target/Contents" -type f | awk '{ print length, $0 }' | sort -rn | cut -d' ' -f2- | while read -r file; do
    [ "$file" = "$sign_target/Contents/MacOS/alcyo" ] && continue
    if file -b "$file" | grep -q 'Mach-O'; then
      codesign --force --timestamp --options runtime --entitlements "$entitlements" --sign "$identity" "$file"
    fi
  done
  codesign --force --timestamp --options runtime --entitlements "$entitlements" --sign "$identity" "$sign_target"
  codesign --verify --strict --deep --verbose=2 "$sign_target"
}

# A disk image to drag the app out of, beside a link to Applications; signed, and notarized if asked.
make_dmg() {
  dmg_app=$1 dmg=$2
  stage=$(mktemp -d)
  ditto "$dmg_app" "$stage/Alcyo.app"
  ln -s /Applications "$stage/Applications"
  hdiutil create -quiet -volname Alcyo -srcfolder "$stage" -ov -format UDZO "$dmg"
  rm -rf "$stage"
  codesign --force --timestamp --sign "$identity" "$dmg"
  if [ "${NOTARIZE:-0}" = 1 ]; then
    key=$(mktemp -d)/AuthKey.p8
    printf '%s' "$APPLE_API_KEY_P8" | base64 --decode > "$key"
    xcrun notarytool submit "$dmg" --key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID" --wait --timeout 2h --output-format json > "$dmg.notary.json" || true
    cat "$dmg.notary.json"
    status=$(sed -n 's/.*"status" *: *"\([^"]*\)".*/\1/p' "$dmg.notary.json" | head -1)
    if [ "$status" != Accepted ]; then
      id=$(sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' "$dmg.notary.json" | head -1)
      [ -z "$id" ] || xcrun notarytool log "$id" --key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID" || true
      rm -f "$key"
      echo "notarization of $dmg: ${status:-failed}" >&2
      exit 1
    fi
    rm -f "$key" "$dmg.notary.json"
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
  fi
}
