# Gcode Viewer

A standalone 3D G-code viewer for Linux, written in Rust with egui and wgpu. Open a `.gcode` file and orbit the toolpath, scrub through layers and switch between fast flat lines and lit, shadowed rendering.

## Features

- 3D toolpath with orbit, pan and zoom camera
- Layer range sliders (top and bottom layer)
- Optional travel move display
- Three render modes: Fast (flat lines, best for large models), Shaded, and Shaded with shadows
- Adjustable light direction and height
- Parses G0/G1 moves, G2/G3 arcs, relative and absolute positioning and extrusion, and G92 resets
- File manager integration: default opener for `.gcode`, a file type icon, and thumbnails taken from the preview image your slicer embeds in the file

## Build requirements

- Rust (stable, edition 2024)
- A Vulkan capable GPU and driver
- On Fedora: `sudo dnf install rust cargo libxkbcommon-devel wayland-devel`

## Build and run

```bash
cargo run --release -- path/to/file.gcode
```

You can also run it without arguments and use "Open file…", or drop a file onto the window.

## Install

```bash
./scripts/install.sh
```

This builds the project and installs:

- `~/.local/bin/gcode-viewer`
- a desktop entry, app icon and `.gcode` file icon under `~/.local/share`
- a MIME definition for `text/x.gcode`, and sets Gcode Viewer as its default application

### Thumbnails

GNOME runs thumbnailers in a sandbox that can only see `/usr`, so the thumbnailer binary has to be installed system wide. The install script prints the exact command if it is missing:

```bash
sudo install -Dm755 target/release/gcode-thumbnailer /usr/local/bin/gcode-thumbnailer
```

Thumbnails only appear for files that contain an embedded preview image (PrusaSlicer, OrcaSlicer and ElegooSlicer all write one). Restart your file manager after installing so it picks up the new thumbnailer.

## Controls

| Input | Action |
|-------|--------|
| Left drag | Orbit |
| Right or middle drag | Pan |
| Scroll | Zoom |

## Limitations

- Layers are detected by Z changes on extruding moves, so vase mode prints show as a single layer
- The whole file is read into memory
- Shaded modes draw every extrusion as geometry and can be slow on very large files, use Fast mode there

## License

GPL-3.0-only. See [LICENSE](LICENSE).
