# Bad Apple in Rust Terminal

A terminal video player written in Rust that renders the "Bad Apple!!" animation using ASCII characters with synchronized audio playback.

## Features

- Synchronized audio and video playback at 30 FPS
- Centered rendering dynamically adjusted to the terminal window size
- Pre-built binary frame data included for immediate playback
- Frame conversion tool to generate binary frame data from PNG sequences

## Prerequisites

### Linux

Building audio support on Linux requires ALSA development libraries and pkg-config:

- Debian / Ubuntu: `sudo apt-get install -y libasound2-dev pkg-config`
- Fedora / RHEL: `sudo dnf install -y alsa-lib-devel pkgconf-pkg-config`
- Arch Linux: `sudo pacman -S alsa-lib pkgconf`

### macOS and Windows

No additional system dependencies are required beyond the standard Rust toolchain.

## Usage

### Play Animation

Play the animation using the included dataset and audio:

```bash
cargo run --release -- play
```

You can also provide custom file paths:

```bash
cargo run --release -- play --input bad_apple.bin --audio audio.ogg
```

### Build Binary Frames (Optional)

If you have extracted frame PNG files in a directory:

```bash
cargo run --release -- build --frames-dir frames --output bad_apple.bin
```

The build command expects PNG files named in sequential order (`frame_0001.png`, `frame_0002.png`, etc.) at 80x60 resolution.

## Terminal Size

For best results without line wrapping or scrolling, resize your terminal window to at least 80 columns by 60 rows before running the player.

## License

This project is licensed under the MIT License. See the LICENSE file for details.
