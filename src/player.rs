use crate::render::{FrameRenderer, HEIGHT, WIDTH};
use crate::terminal::TerminalGuard;
use crossterm::{
    cursor::MoveTo,
    event::{Event, KeyCode, KeyModifiers, poll, read},
    execute,
    terminal::{Clear, ClearType, size},
};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub const MIN_FPS: f64 = 0.001;
pub const DEFAULT_FPS: f64 = 30.0;

pub fn normalize_fps(fps: f64) -> f64 {
    if !fps.is_finite() || fps < MIN_FPS {
        DEFAULT_FPS
    } else {
        fps
    }
}

pub fn compute_frame_duration(fps: f64) -> Duration {
    let effective_fps = normalize_fps(fps);
    Duration::from_secs_f64(1.0 / effective_fps)
}

pub fn play(input: &str, audio_path: &str, fps: f64) -> Result<(), Box<dyn std::error::Error>> {
    play_with_cancel(input, audio_path, fps, &crate::INTERRUPTED)
}

pub fn play_with_cancel(
    input: &str,
    audio_path: &str,
    fps: f64,
    interrupted: &AtomicBool,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame_duration = compute_frame_duration(fps);

    if interrupted.load(Ordering::Relaxed) {
        return Err("Playback aborted by user interrupt".into());
    }

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
    let mut renderer = FrameRenderer::new();

    #[cfg(feature = "audio")]
    if let Some((_, ref sink)) = _audio_handle {
        sink.play();
    }

    let start_time = Instant::now();
    let mut frame_count = 0;

    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Err("Playback aborted by user interrupt".into());
        }

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

                let output = renderer.render(&buffer, current_size.0, current_size.1);

                execute!(stdout, MoveTo(0, 0))?;
                print!("{output}");
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_play_cleanup_on_interrupt() {
        let interrupted = AtomicBool::new(true);
        let result = play_with_cancel("nonexistent_bad_apple.bin", "audio.ogg", 30.0, &interrupted);
        assert!(result.is_err(), "Interrupt signal must abort play");
        assert_eq!(
            result.unwrap_err().to_string(),
            "Playback aborted by user interrupt"
        );
    }
}
