#!/bin/sh
# Wait for Apple to notarize the disk images package.sh sent with NOTARIZE=1 NOTARY_WAIT=0, as listed in
# target/package/notary-pending.txt beside them, and staple each one into target/package/notarized/.
# Run again after a timeout, it asks after the same submissions; nothing is built or sent again.
# Needs the App Store Connect API key (see packaging/macos/sign.sh) and NOTARY_HOURS, by default 3.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
. packaging/macos/sign.sh
[ -s "$notary_list" ] || { echo "nothing sent to Apple: no $notary_list" >&2; exit 1; }
cat "$notary_list"
notary_wait
ls -lh target/package/notarized
