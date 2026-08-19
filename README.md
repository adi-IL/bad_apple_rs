# Bad Apple Terminal Player in Rust

A lightweight ASCII art video and audio player in Rust that renders the Bad Apple animation directly inside your terminal with optional synchronized audio playback.

## Quick Start

### Play Immediately (Zero C Dependencies)

Clone the repository and run the player directly:

```bash
git clone https://github.com/adi-IL/bad_apple_rs.git
cd bad_apple_rs
cargo run --release -- play
```

For the best visual presentation, resize your terminal window to at least 80 columns by 60 rows.

### Play with Audio Output

To enable audio playback with `rodio`, build with the `audio` feature:

```bash
cargo run --release --features audio -- play
```

> Note for Linux users: Audio support requires ALSA development headers:
> `sudo apt-get install -y libasound2-dev pkg-config` (Ubuntu/Debian) or `sudo dnf install -y alsa-lib-devel pkgconf-pkg-config` (Fedora).

## Features

- High performance ASCII rendering at 30 frames per second
- In-memory double-buffered rendering to eliminate terminal flickering
- Dynamic viewport centering based on current terminal dimensions
- Standalone offline playback with pre-encoded binary assets included
- Pure Rust default build requiring zero external C system libraries
- Optional audio playback engine powered by Rodio
- Built-in frame conversion tool to generate binary assets from raw image sequences

## CLI Usage

### Play Animation

Play the default animation:

```bash
cargo run --release -- play
```

Play with audio enabled:

```bash
cargo run --release --features audio -- play
```

Specify custom frame binary or audio files:

```bash
cargo run --release -- play --input custom_frames.bin
cargo run --release --features audio -- play --input custom_frames.bin --audio custom_audio.ogg
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

## License

This project is distributed under the MIT License. See the LICENSE file for complete terms.
