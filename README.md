# Bad Apple Terminal Player in Rust

A lightweight ASCII art video and audio player in Rust that renders the Bad Apple animation directly inside your terminal with optional synchronized audio playback.

## Quick Start

### Play Immediately (with Audio)

Clone the repository and run the player directly:

```bash
git clone https://github.com/adi-IL/bad_apple_rs.git
cd bad_apple_rs
cargo run --release -- play
```

For the best visual presentation, resize your terminal window to at least 80 columns by 60 rows.

> Note for Linux users: Audio playback uses ALSA development headers:
> `sudo apt-get install -y libasound2-dev pkg-config` (Ubuntu/Debian) or `sudo dnf install -y alsa-lib-devel pkgconf-pkg-config` (Fedora).

### Play Without Audio (Zero C Dependencies)

To run pure Rust video playback without external audio dependencies:

```bash
cargo run --release --no-default-features -- play
```

## Features

- High performance ASCII rendering at 30 frames per second
- Monotonic clock pacing with frame dropping to eliminate audio and video drift
- Allocation-free frame rendering using persistent reusable buffers
- Panic hook and signal handling that guarantee terminal restoration
- Dynamic viewport centering based on current terminal dimensions
- Sequence gap detection during frame compilation
- Standalone offline playback with pre-encoded binary assets included
- Synchronized audio playback engine powered by Rodio enabled by default
- Pure Rust video-only build option with zero external C system libraries
- Built-in frame conversion tool to generate binary assets from raw image sequences
## CLI Usage

### Play Animation

Play with synchronized audio:

```bash
cargo run --release -- play
```

Play in video-only mode:

```bash
cargo run --release --no-default-features -- play
```

Specify custom frame binary or audio files:

```bash
cargo run --release -- play --input custom_frames.bin
cargo run --release -- play --input custom_frames.bin --audio custom_audio.ogg
```

### Build Frames Binary (Optional)

Convert a directory of PNG frames into a single binary file:

```bash
cargo run --release -- build --frames-dir frames --output bad_apple.bin
```

The build command reads sequential PNG files (such as `frame_0001.png`, `frame_0002.png`) scaled to 80x60 pixels and encodes them using an ASCII luminance ramp (` .:-=+*#%@`).

## CLI Options

| Command | Option | Default | Description |
|---|---|---|---|
| `play` | `--input, -i` | `bad_apple.bin` | Path to the encoded ASCII frames binary file |
| `play` | `--audio, -a` | `audio.ogg` | Path to the audio soundtrack file |
| `play` | `--fps` | `30.0` | Target playback frame rate in frames per second |
| `build` | `--frames-dir, -f` | `frames` | Directory containing sequential PNG frame files |
| `build` | `--output, -o` | `bad_apple.bin` | Output path for the generated binary file |

## Technical Architecture

```text
[ PNG Frame Sequence ] ---> [ build ] ---> [ bad_apple.bin (ASCII byte stream) ]
                                                   |
[ audio.ogg (Vorbis) ]  ---> [ play  ] <-----------+
                                   |
                         +---------+---------+
                         |                   |
                         v                   v
                 [ Rodio Audio ]    [ Crossterm TUI (MoveTo 0,0) ]
```

### Subsystem Modules

- `src/cli.rs`. Command-line arguments and subcommand definitions using clap.
- `src/terminal.rs`. RAII terminal guard, raw mode management, and panic hook restoration.
- `src/render.rs`. ASCII luminance mapping, viewport padding, and allocation-free frame rendering.
- `src/player.rs`. Monotonic clock pacing, frame-dropping drift elimination, and playback event loop.
- `src/builder.rs`. Image sequence ingestion, gap detection, resizing, and atomic binary frame output.

## License

This project is distributed under the MIT License. See the LICENSE file for complete terms.
