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
  [ "${NOTARIZE:-0}" = 1 ] && notary_submit "$dmg"
  return 0
}

# Notarizing: every disk image is sent to Apple as soon as it is made, and its submission noted in
# target/package/notary-pending.txt. notary_wait then waits for all of them together, either at the end of
# package.sh or later in a job of its own (notarize-wait.sh): Apple's queue has taken from minutes to
# several hours, and the noted submissions let a later wait pick up where an earlier one stopped, without
# building or sending anything again. Each step retries, as long waits meet the odd network outage.
notary_list="$root/target/package/notary-pending.txt"
notary_key=""

notary() {
  if [ -z "$notary_key" ]; then
    notary_key=$(mktemp -d)/AuthKey.p8
    printf '%s' "$APPLE_API_KEY_P8" | base64 --decode > "$notary_key"
  fi
  xcrun notarytool "$@" --key "$notary_key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID"
}

json_field() {
  sed -n "s/.*\"$1\" *: *\"\([^\"]*\)\".*/\1/p" | head -1
}

notary_submit() {
  submit_dmg=$1
  for attempt in 1 2 3 4 5; do
    submit_id=$(notary submit "$submit_dmg" --output-format json 2>/dev/null | json_field id || true)
    [ -n "$submit_id" ] && break
    echo "sending $submit_dmg to Apple failed (attempt $attempt), trying again" >&2
    sleep 30
  done
  [ -n "$submit_id" ] || { echo "could not send $submit_dmg to Apple" >&2; exit 1; }
  echo "sent $submit_dmg to Apple: $submit_id"
  # By file name, so the list still holds wherever the images are unpacked again.
  printf '%s %s\n' "$submit_id" "$(basename "$submit_dmg")" >> "$notary_list"
}

# Wait for every noted submission, at most NOTARY_HOURS (default 3), stapling each ticket as it comes and
# moving that image to target/package/notarized/. A rejection prints Apple's log and fails.
notary_wait() {
  [ -s "$notary_list" ] || return 0
  hours=${NOTARY_HOURS:-3}
  deadline=$(( $(date +%s) + hours * 3600 ))
  done_dir="$(dirname "$notary_list")/notarized"
  mkdir -p "$done_dir"
  left=$(awk '{ print $1 "=" $2 }' "$notary_list")
  while [ -n "$left" ]; do
    [ "$(date +%s)" -lt "$deadline" ] || { echo "Apple did not finish notarizing within $hours hours:" $left >&2; rm -f "$notary_key"; exit 1; }
    still=""
    for entry in $left; do
      wait_id=${entry%%=*} wait_dmg="$(dirname "$notary_list")/${entry#*=}"
      # A failed check, from the network or Apple, is asked again next time round.
      wait_status=$(notary info "$wait_id" --output-format json 2>/dev/null | json_field status || true)
      case "$wait_status" in
        Accepted)
          echo "notarized $wait_dmg"
          xcrun stapler staple "$wait_dmg"
          xcrun stapler validate "$wait_dmg"
          mv "$wait_dmg" "$done_dir/"
          ;;
        Invalid|Rejected)
          notary log "$wait_id" || true
          rm -f "$notary_key"
          echo "notarization of $wait_dmg: $wait_status" >&2
          exit 1
          ;;
        *) still="$still $entry" ;;
      esac
    done
    left=$still
    [ -z "$left" ] || sleep 30
  done
  rm -f "$notary_key" "$notary_list"
}
