#!/bin/sh
# Install Alcyo from this package, or remove it again.
#
#   ./install.sh                  # into ~/.local, no root needed
#   ./install.sh --uninstall      # removes the app; your meetings and models stay
#   sudo PREFIX=/usr/local ./install.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
PREFIX=${PREFIX:-"$HOME/.local"}
BINDIR="$PREFIX/bin"
LIBDIR="$PREFIX/lib/alcyo"
APPDIR="$PREFIX/share/applications"
ICONDIR="$PREFIX/share/icons/hicolor/256x256/apps"
LICENSEDIR="$PREFIX/share/licenses/alcyo"

refresh() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$APPDIR" >/dev/null 2>&1 || true
    fi
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BINDIR/alcyo" "$APPDIR/app.alcyo.board.desktop" "$ICONDIR/app.alcyo.board.png"
    rm -rf "$LIBDIR" "$LICENSEDIR"
    refresh
    echo "Removed Alcyo from $PREFIX. Your meetings and models were kept."
    exit 0
fi

install -d "$BINDIR" "$APPDIR" "$ICONDIR" "$PREFIX/lib" "$LICENSEDIR"
# Replace the resources whole, so nothing from an older version lingers.
rm -rf "$LIBDIR"
cp -r "$here/lib/alcyo" "$LIBDIR"
install -m755 "$here/bin/alcyo" "$BINDIR/alcyo"
install -m644 "$here/share/app.alcyo.board.png" "$ICONDIR/app.alcyo.board.png"
cp "$here"/share/licenses/alcyo/* "$LICENSEDIR/"
sed -e "s|@BINDIR@|$BINDIR|" -e "s|@VERSION@|$(cat "$here/VERSION")|" "$here/share/app.alcyo.board.desktop.in" > "$APPDIR/app.alcyo.board.desktop"
chmod 644 "$APPDIR/app.alcyo.board.desktop"
refresh

"$BINDIR/alcyo" --self-check >/dev/null
echo "Installed Alcyo $(cat "$here/VERSION") to $PREFIX."
case ":$PATH:" in
    *":$BINDIR:"*) ;;
    *) echo "$BINDIR is not on your PATH. The app is in your launcher; to use the command, add it to PATH." ;;
esac
