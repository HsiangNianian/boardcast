#!/bin/sh
# Install boardcast from this package, or remove it again.
#
#   ./install.sh                  # into ~/.local, no root needed
#   ./install.sh --uninstall      # removes the app; your meetings and models stay
#   sudo PREFIX=/usr/local ./install.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
PREFIX=${PREFIX:-"$HOME/.local"}
BINDIR="$PREFIX/bin"
LIBDIR="$PREFIX/lib/boardcast"
APPDIR="$PREFIX/share/applications"
ICONDIR="$PREFIX/share/icons/hicolor/256x256/apps"
LICENSEDIR="$PREFIX/share/licenses/boardcast"

refresh() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$APPDIR" >/dev/null 2>&1 || true
    fi
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BINDIR/boardcast" "$APPDIR/boardcast.desktop" "$ICONDIR/boardcast.png"
    rm -rf "$LIBDIR" "$LICENSEDIR"
    refresh
    echo "Removed boardcast from $PREFIX. Your meetings and models were kept."
    exit 0
fi

install -d "$BINDIR" "$APPDIR" "$ICONDIR" "$PREFIX/lib" "$LICENSEDIR"
# Replace the resources whole, so nothing from an older version lingers.
rm -rf "$LIBDIR"
cp -r "$here/lib/boardcast" "$LIBDIR"
install -m755 "$here/bin/boardcast" "$BINDIR/boardcast"
install -m644 "$here/share/boardcast.png" "$ICONDIR/boardcast.png"
cp "$here"/share/licenses/boardcast/* "$LICENSEDIR/"
sed -e "s|@BINDIR@|$BINDIR|" -e "s|@VERSION@|$(cat "$here/VERSION")|" "$here/share/boardcast.desktop.in" > "$APPDIR/boardcast.desktop"
chmod 644 "$APPDIR/boardcast.desktop"
refresh

"$BINDIR/boardcast" --self-check >/dev/null
echo "Installed boardcast $(cat "$here/VERSION") to $PREFIX."
case ":$PATH:" in
    *":$BINDIR:"*) ;;
    *) echo "$BINDIR is not on your PATH. The app is in your launcher; to use the command, add it to PATH." ;;
esac
