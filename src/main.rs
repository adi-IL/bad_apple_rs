use clap::{Parser, Subcommand};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{poll, read, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the binary frames file from images
    Build {
        #[arg(short, long, default_value = "frames")]
        frames_dir: String,
        #[arg(short, long, default_value = "bad_apple.bin")]
        output: String,
    },
    /// Play the animation
    Play {
        #[arg(short, long, default_value = "bad_apple.bin")]
        input: String,
        #[arg(short, long, default_value = "audio.ogg")]
        audio: String,
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
    },
}

const WIDTH: u32 = 80;
const HEIGHT: u32 = 60;
const ASCII_CHARS: &[u8] = b" .:-=+*#%@";

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Self {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All));
        Self
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
    }
}

fn pixel_to_ascii(pixel: u8) -> u8 {
    let idx = (pixel as usize * (ASCII_CHARS.len() - 1)) / 255;
    ASCII_CHARS[idx]
}

fn compute_padding(term_width: u16, term_height: u16, frame_w: u32, frame_h: u32) -> (u16, u16) {
    let pad_x = if term_width > frame_w as u16 {
        (term_width - frame_w as u16) / 2
    } else {
        0
    };
    let pad_y = if term_height > frame_h as u16 {
        (term_height - frame_h as u16) / 2
    } else {
        0
    };
    (pad_x, pad_y)
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Build { frames_dir, output } => {
            build_frames(frames_dir, output);
        }
        Commands::Play { input, audio, fps } => {
            play(input, audio, *fps);
        }
    }
}

fn build_frames(frames_dir: &str, output: &str) {
    let mut out_file = BufWriter::new(File::create(output).unwrap());
    let mut i = 1;
    loop {
        let frame_path = format!("{}/frame_{:04}.png", frames_dir, i);
        if !Path::new(&frame_path).exists() {
            println!("Finished processing {} frames.", i - 1);
            break;
        }

        let img = image::open(&frame_path).unwrap();
        // The image is already 80x60
        let gray = img.to_luma8();
        let mut frame_data = Vec::with_capacity((WIDTH * HEIGHT) as usize);

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let pixel = gray.get_pixel(x, y)[0];
                frame_data.push(pixel_to_ascii(pixel));
            }
        }
        out_file.write_all(&frame_data).unwrap();
        i += 1;
    }
}

fn play(input: &str, audio_path: &str, fps: f64) {
    let file = File::open(input).expect("Could not open frames binary file");
    let mut reader = BufReader::new(file);

    #[cfg(feature = "audio")]
    let _audio_handle = {
        match rodio::OutputStream::try_default() {
            Ok((stream, stream_handle)) => match rodio::Sink::try_new(&stream_handle) {
                Ok(sink) => {
                    if let Ok(audio_file) = File::open(audio_path) {
                        let audio_reader = BufReader::new(audio_file);
                        if let Ok(decoder) = rodio::Decoder::new(audio_reader) {
                            sink.append(decoder);
                            sink.play();
                            Some((stream, sink))
                        } else {
                            eprintln!("Failed to decode audio");
                            None
                        }
                    } else {
                        None
                    }
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    };

    #[cfg(not(feature = "audio"))]
    let _ = audio_path;

    let _guard = TerminalGuard::new();
    let mut stdout = std::io::stdout();

    let frame_size = (WIDTH * HEIGHT) as usize;
    let mut buffer = vec![0u8; frame_size];

    let effective_fps = if fps <= 0.0 { 30.0 } else { fps };
    let frame_duration = Duration::from_secs_f64(1.0 / effective_fps);
    let start_time = Instant::now();
    let mut frame_count = 0;

    thread::sleep(Duration::from_millis(500));

    loop {
        if poll(Duration::from_millis(0)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = read() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => {}
                }
            }
        }

        match reader.read_exact(&mut buffer) {
            Ok(_) => {
                let (term_width, term_height) = size().unwrap_or((80, 60));
                let (pad_x, pad_y) = compute_padding(term_width, term_height, WIDTH, HEIGHT);

                let mut output =
                    String::with_capacity(frame_size + (term_height as usize * term_width as usize));

                for _ in 0..pad_y {
                    output.push('\n');
                }

                for y in 0..HEIGHT {
                    for _ in 0..pad_x {
                        output.push(' ');
                    }

                    let start = (y * WIDTH) as usize;
                    let end = start + WIDTH as usize;
                    let line = std::str::from_utf8(&buffer[start..end]).unwrap_or("");
                    output.push_str(line);
                    output.push('\n');
                }

                execute!(stdout, MoveTo(0, 0)).unwrap();
                print!("{}", output);
                stdout.flush().unwrap();

                frame_count += 1;

                let expected_time = frame_duration * frame_count;
                let elapsed = start_time.elapsed();
                if expected_time > elapsed {
                    thread::sleep(expected_time - elapsed);
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_to_ascii_extremes() {
        assert_eq!(pixel_to_ascii(0), b' ');
        assert_eq!(pixel_to_ascii(255), b'@');
    }

    #[test]
    fn test_compute_padding_centered() {
        let (pad_x, pad_y) = compute_padding(100, 80, 80, 60);
        assert_eq!(pad_x, 10);
        assert_eq!(pad_y, 10);
    }

    #[test]
    fn test_compute_padding_clamped_at_zero() {
        let (pad_x, pad_y) = compute_padding(60, 40, 80, 60);
        assert_eq!(pad_x, 0);
        assert_eq!(pad_y, 0);
    }
}
