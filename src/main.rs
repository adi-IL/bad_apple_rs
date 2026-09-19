use clap::{Parser, Subcommand};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{Event, KeyCode, KeyModifiers, poll, read},
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    fn new() -> Result<Self, std::io::Error> {
        let mut stdout = std::io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        let guard = Self;
        if let Err(err) = execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All)) {
            drop(guard);
            return Err(err);
        }
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
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

struct TempFileCleanup<'a>(&'a Path, bool);

impl<'a> Drop for TempFileCleanup<'a> {
    fn drop(&mut self) {
        if !self.1 {
            let _ = std::fs::remove_file(self.0);
        }
    }
}

const MIN_FPS: f64 = 0.001;
const DEFAULT_FPS: f64 = 30.0;

fn normalize_fps(fps: f64) -> f64 {
    if !fps.is_finite() || fps < MIN_FPS {
        DEFAULT_FPS
    } else {
        fps
    }
}

fn compute_frame_duration(fps: f64) -> Duration {
    let effective_fps = normalize_fps(fps);
    Duration::from_secs_f64(1.0 / effective_fps)
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

fn init_signal_handler() {
    let _ = ctrlc::set_handler(|| {
        if INTERRUPTED.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
    });
}

fn main() {
    init_signal_handler();
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Build { frames_dir, output } => build_frames(frames_dir, output),
        Commands::Play { input, audio, fps } => play(input, audio, *fps),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        if INTERRUPTED.load(Ordering::Relaxed) {
            std::process::exit(130);
        }
        std::process::exit(1);
    }
}

fn build_frames(frames_dir: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let first_frame = format!("{frames_dir}/frame_0001.png");
    if !Path::new(&first_frame).exists() {
        return Err(format!("No frames found in directory: {frames_dir}").into());
    }

    let output_path = Path::new(output);
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let file_name = output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output.bin");

    let temp_output = match output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => parent.join(format!(".tmp_{}_{}", std::process::id(), file_name)),
        None => PathBuf::from(format!(".tmp_{}_{}", std::process::id(), file_name)),
    };

    let mut cleanup = TempFileCleanup(&temp_output, false);
    let out_file = File::create(&temp_output)?;
    let mut out_file = BufWriter::new(out_file);
    let mut i = 1;
    let mut processed = 0;

    loop {
        if INTERRUPTED.load(Ordering::Relaxed) {
            return Err("Build aborted by user interrupt".into());
        }

        let frame_path = format!("{}/frame_{:04}.png", frames_dir, i);
        if !Path::new(&frame_path).exists() {
            break;
        }

        let img = image::open(&frame_path)?;
        let img = if img.width() != WIDTH || img.height() != HEIGHT {
            img.resize_exact(WIDTH, HEIGHT, image::imageops::FilterType::Nearest)
        } else {
            img
        };
        let gray = img.to_luma8();
        let mut frame_data = Vec::with_capacity((WIDTH * HEIGHT) as usize);

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let pixel = gray.get_pixel(x, y)[0];
                frame_data.push(pixel_to_ascii(pixel));
            }
        }
        out_file.write_all(&frame_data)?;
        i += 1;
        processed += 1;
    }

    out_file.flush()?;
    out_file.get_ref().sync_all()?;
    drop(out_file);
    std::fs::rename(&temp_output, output)?;
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = File::open(parent).and_then(|dir| dir.sync_all());
    }
    cleanup.1 = true;

    println!("Finished processing {processed} frames.");
    Ok(())
}

fn play(input: &str, audio_path: &str, fps: f64) -> Result<(), Box<dyn std::error::Error>> {
    let frame_duration = compute_frame_duration(fps);

    let file = File::open(input)
        .map_err(|e| format!("Could not open frames binary file '{input}': {e}"))?;
    let mut reader = BufReader::new(file);

    #[cfg(feature = "audio")]
    let _audio_handle = {
        match rodio::OutputStream::try_default() {
            Ok((stream, stream_handle)) => match rodio::Sink::try_new(&stream_handle) {
                Ok(sink) => {
                    sink.pause();
                    if let Ok(audio_file) = File::open(audio_path) {
                        let audio_reader = BufReader::new(audio_file);
                        if let Ok(decoder) = rodio::Decoder::new(audio_reader) {
                            sink.append(decoder);
                            Some((stream, sink))
                        } else {
                            eprintln!("Failed to decode audio");
                            None
                        }
                    } else {
                        eprintln!("Audio file not found, playing without audio");
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

    let _guard = TerminalGuard::new()?;
    let mut stdout = std::io::stdout();

    let frame_size = (WIDTH * HEIGHT) as usize;
    let mut buffer = vec![0u8; frame_size];
    let mut last_size = size().unwrap_or((80, 60));

    #[cfg(feature = "audio")]
    if let Some((_, ref sink)) = _audio_handle {
        sink.play();
    }

    let start_time = Instant::now();
    let mut frame_count = 0;

    loop {
        while poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        match reader.read_exact(&mut buffer) {
            Ok(_) => {
                let current_size = size().unwrap_or((80, 60));
                if current_size != last_size {
                    let _ = execute!(stdout, Clear(ClearType::All));
                    last_size = current_size;
                }

                let output = render_frame(&buffer, current_size.0, current_size.1);

                execute!(stdout, MoveTo(0, 0))?;
                print!("{}", output);
                stdout.flush()?;

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

    Ok(())
}

fn render_frame(buffer: &[u8], term_width: u16, term_height: u16) -> String {
    let frame_size = (WIDTH * HEIGHT) as usize;
    let (pad_x, pad_y) = compute_padding(term_width, term_height, WIDTH, HEIGHT);

    let mut output =
        String::with_capacity(frame_size + (term_height as usize * term_width as usize));

    for _ in 0..pad_y {
        output.push_str("\r\n");
    }

    for y in 0..HEIGHT {
        for _ in 0..pad_x {
            output.push(' ');
        }

        let start = (y * WIDTH) as usize;
        let end = start + WIDTH as usize;
        let line = std::str::from_utf8(&buffer[start..end]).unwrap_or("");
        output.push_str(line);
        output.push_str("\r\n");
    }

    output
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

    #[test]
    fn test_render_frame_contains_carriage_returns() {
        let dummy = vec![b' '; (WIDTH * HEIGHT) as usize];
        let frame = render_frame(&dummy, 80, 60);
        assert!(
            frame.contains("\r\n"),
            "Rendered frames in raw mode must contain \\r\\n"
        );
    }

    #[test]
    fn test_play_missing_file_returns_error() {
        let result = play("nonexistent_bad_apple.bin", "audio.ogg", 30.0);
        assert!(result.is_err(), "Missing frames binary must return an Err");
    }

    #[test]
    fn test_normalize_fps_and_duration() {
        assert_eq!(normalize_fps(f64::NAN), 30.0);
        assert_eq!(normalize_fps(f64::INFINITY), 30.0);
        assert_eq!(normalize_fps(f64::NEG_INFINITY), 30.0);
        assert_eq!(normalize_fps(0.0), 30.0);
        assert_eq!(normalize_fps(-10.0), 30.0);
        assert_eq!(normalize_fps(1e-25), 30.0);
        assert_eq!(normalize_fps(0.0001), 30.0);
        assert_eq!(normalize_fps(30.0), 30.0);
        assert_eq!(normalize_fps(60.0), 60.0);

        let d_nan = compute_frame_duration(f64::NAN);
        assert_eq!(d_nan, Duration::from_secs_f64(1.0 / 30.0));

        let d_tiny = compute_frame_duration(1e-40);
        assert_eq!(d_tiny, Duration::from_secs_f64(1.0 / 30.0));
    }

    #[test]
    fn test_play_nan_fps_does_not_panic() {
        let result = play("nonexistent_bad_apple.bin", "audio.ogg", f64::NAN);
        assert!(
            result.is_err(),
            "NaN fps must not panic and return Err on missing file"
        );

        let result_tiny = play("nonexistent_bad_apple.bin", "audio.ogg", 1e-40);
        assert!(
            result_tiny.is_err(),
            "Subnormal fps must not panic and return Err on missing file"
        );
    }

    #[test]
    fn test_build_missing_frames_returns_error() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_missing_frames_{}", std::process::id()));
        let out_file = temp_dir.join("out.bin");
        let result = build_frames(
            "nonexistent_frames_directory_xyz",
            out_file.to_str().unwrap(),
        );
        assert!(
            result.is_err(),
            "Missing frames directory must return an Err"
        );
        assert!(
            !out_file.exists(),
            "Output file must not be created on error"
        );
    }

    #[test]
    fn test_build_frames_does_not_truncate_existing_file_on_error() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_truncate_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let out_file = temp_dir.join("existing.bin");
        std::fs::write(&out_file, b"preserve me").unwrap();

        let result = build_frames("nonexistent_frames_dir_123", out_file.to_str().unwrap());
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(&out_file).unwrap(),
            b"preserve me",
            "Existing output file must not be truncated when frames are missing"
        );

        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();

        let valid_img = image::GrayImage::from_pixel(80, 60, image::Luma([128]));
        valid_img.save(frames_dir.join("frame_0001.png")).unwrap();
        std::fs::write(frames_dir.join("frame_0002.png"), b"not a valid png file").unwrap();

        let result_mid = build_frames(frames_dir.to_str().unwrap(), out_file.to_str().unwrap());
        assert!(result_mid.is_err(), "Corrupt frame must return an Err");
        assert_eq!(
            std::fs::read(&out_file).unwrap(),
            b"preserve me",
            "Existing output file must not be truncated when mid-processing error occurs"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_build_frames_resizes_non_standard_image() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_test_frames_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let img = image::GrayImage::from_pixel(160, 120, image::Luma([128]));
        img.save(temp_dir.join("frame_0001.png")).unwrap();

        let out_file = temp_dir.join("out.bin");
        let result = build_frames(temp_dir.to_str().unwrap(), out_file.to_str().unwrap());
        assert!(
            result.is_ok(),
            "build_frames should resize and convert non-80x60 images"
        );

        let data = std::fs::read(&out_file).unwrap();
        assert_eq!(data.len(), (WIDTH * HEIGHT) as usize);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_build_frames_cleanup_on_interrupt() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_interrupt_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();
        let valid_img = image::GrayImage::from_pixel(80, 60, image::Luma([128]));
        valid_img.save(frames_dir.join("frame_0001.png")).unwrap();

        let out_file = temp_dir.join("out.bin");

        // Simulate interrupt signal
        INTERRUPTED.store(true, Ordering::SeqCst);
        let result = build_frames(frames_dir.to_str().unwrap(), out_file.to_str().unwrap());
        INTERRUPTED.store(false, Ordering::SeqCst);

        assert!(result.is_err(), "Interrupt signal must abort build");
        assert!(
            !out_file.exists(),
            "Output file must not be created on interrupt"
        );

        for entry in std::fs::read_dir(&temp_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with(".tmp_"),
                "Temp file was not cleaned up: {name}"
            );
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
