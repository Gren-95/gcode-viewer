#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "Usage: $0 [--all]"
    echo "  (no flag)  install the viewer, icons, MIME type and default handler"
    echo "  --all      also install the thumbnailer (needs sudo)"
}

install_thumbnailer=false
case "${1:-}" in
    "") ;;
    --all) install_thumbnailer=true ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
esac

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
bin_dir="$HOME/.local/bin"
# GNOME runs thumbnailers in a sandbox that only sees /usr, so this binary must live there.
thumbnailer_path="/usr/local/bin/gcode-thumbnailer"

cd "$project_dir"
cargo build --release

install -Dm755 target/release/gcode-viewer "$bin_dir/gcode-viewer"
mkdir -p "$data_dir/applications"
sed "s|@BIN_PATH@|$bin_dir/gcode-viewer|" packaging/gcode-viewer.desktop > "$data_dir/applications/gcode-viewer.desktop"
install -Dm644 packaging/gcode-viewer.svg "$data_dir/icons/hicolor/scalable/apps/gcode-viewer.svg"
install -Dm644 packaging/gcode-viewer-file.svg "$data_dir/icons/hicolor/scalable/mimetypes/gcode-viewer-file.svg"
install -Dm644 packaging/text-x-gcode.xml "$data_dir/mime/packages/text-x-gcode.xml"

update-mime-database "$data_dir/mime"
update-desktop-database "$data_dir/applications"
xdg-mime default gcode-viewer.desktop text/x.gcode

if [ "$install_thumbnailer" = true ]; then
    sudo install -Dm755 target/release/gcode-thumbnailer "$thumbnailer_path"
    mkdir -p "$data_dir/thumbnailers"
    sed "s|@THUMBNAILER_PATH@|$thumbnailer_path|g" packaging/gcode-viewer.thumbnailer > "$data_dir/thumbnailers/gcode-viewer.thumbnailer"
    rm -f "$HOME"/.cache/thumbnails/fail/gnome-thumbnail-factory/*
    echo "Installed thumbnailer. Restart your file manager (nautilus -q) to pick it up."
fi

echo "Installed to $bin_dir/gcode-viewer and set as default for text/x.gcode"
