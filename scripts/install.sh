#!/usr/bin/env bash
set -euo pipefail

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
mkdir -p "$data_dir/thumbnailers"
sed "s|@THUMBNAILER_PATH@|$thumbnailer_path|g" packaging/gcode-viewer.thumbnailer > "$data_dir/thumbnailers/gcode-viewer.thumbnailer"
install -Dm644 packaging/gcode-viewer.svg "$data_dir/icons/hicolor/scalable/apps/gcode-viewer.svg"
install -Dm644 packaging/gcode-viewer-file.svg "$data_dir/icons/hicolor/scalable/mimetypes/gcode-viewer-file.svg"
install -Dm644 packaging/text-x-gcode.xml "$data_dir/mime/packages/text-x-gcode.xml"

update-mime-database "$data_dir/mime"
update-desktop-database "$data_dir/applications"
xdg-mime default gcode-viewer.desktop text/x.gcode

if ! cmp -s target/release/gcode-thumbnailer "$thumbnailer_path"; then
    echo "Thumbnails need: sudo install -Dm755 target/release/gcode-thumbnailer $thumbnailer_path"
fi
echo "Installed to $bin_dir/gcode-viewer and set as default for text/x.gcode"
