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
pub const MAX_FPS: f64 = 1_000_000.0;
pub const DEFAULT_FPS: f64 = 30.0;

pub fn normalize_fps(fps: f64) -> f64 {
    if !fps.is_finite() || fps < MIN_FPS || fps > MAX_FPS {
        DEFAULT_FPS
    } else {
        fps
    }
}

pub fn compute_frame_duration(fps: f64) -> Duration {
    let effective_fps = normalize_fps(fps);
    Duration::from_secs_f64(1.0 / effective_fps)
}
#[derive(Debug, Clone)]
pub struct PlaybackClock {
    pub start_time: Instant,
    pub frame_duration: Duration,
}

impl PlaybackClock {
    pub fn new(fps: f64) -> Self {
        let normalized = normalize_fps(fps);
        Self {
            start_time: Instant::now(),
            frame_duration: compute_frame_duration(normalized),
        }
    }

    pub fn target_time(&self, frame: u64) -> Duration {
        self.frame_duration.mul_f64(frame as f64)
    }

    pub fn should_drop_frame(&self, frame: u64) -> bool {
        self.start_time.elapsed() > self.target_time(frame.saturating_add(1))
    }

    pub fn sleep_until_next_frame(&self, frame: u64, interrupted: &AtomicBool) {
        let target = self.target_time(frame);
        while !interrupted.load(Ordering::Relaxed) {
            let elapsed = self.start_time.elapsed();
            if target <= elapsed {
                break;
            }
            let remaining = target - elapsed;
            let step = remaining.min(Duration::from_millis(50));
            thread::sleep(step);
        }
    }
}
pub fn read_frame<R: Read>(
    reader: &mut R,
    buffer: &mut [u8],
    frame_index: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    if buffer.is_empty() {
        return Err("Frame buffer cannot be empty".into());
    }

    let mut first_byte = [0u8; 1];
    match reader.read_exact(&mut first_byte) {
        Ok(()) => {
            buffer[0] = first_byte[0];
            reader.read_exact(&mut buffer[1..]).map_err(|e| {
                format!("Corrupt or truncated frame {frame_index}: {e}")
            })?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(format!("Error reading frame {frame_index}: {e}").into()),
    }
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
    if interrupted.load(Ordering::Relaxed) {
        return Err("Playback aborted by user interrupt".into());
    }

    let file = File::open(input)
        .map_err(|e| format!("Could not open frames binary file '{input}': {e}"))?;
    let mut reader = BufReader::new(file);

    #[cfg(feature = "audio")]
    let (_audio_handle, audio_warning) = {
        match rodio::OutputStream::try_default() {
            Ok((stream, stream_handle)) => match rodio::Sink::try_new(&stream_handle) {
                Ok(sink) => {
                    sink.pause();
                    match File::open(audio_path) {
                        Ok(audio_file) => {
                            let audio_reader = BufReader::new(audio_file);
                            match rodio::Decoder::new(audio_reader) {
                                Ok(decoder) => {
                                    sink.append(decoder);
                                    (Some((stream, sink)), None)
                                }
                                Err(err) => (
                                    None,
                                    Some(format!("Failed to decode audio file '{audio_path}': {err}")),
                                ),
                            }
                        }
                        Err(err) => (
                            None,
                            Some(format!("Could not open audio file '{audio_path}': {err}")),
                        ),
                    }
                }
                Err(err) => (None, Some(format!("Failed to create audio sink: {err}"))),
            },
            Err(err) => (
                None,
                Some(format!("Failed to initialize audio output device: {err}")),
            ),
        }
    };

    #[cfg(not(feature = "audio"))]
    let audio_warning = if audio_path != "audio.ogg" {
        Some(format!(
            "Warning: Custom audio path '{audio_path}' was specified, but this binary was built without the 'audio' feature."
        ))
    } else {
        None
    };
    let play_result = (|| -> Result<(), Box<dyn std::error::Error>> {
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
        let clock = PlaybackClock::new(fps);
        let mut frame_index: u64 = 0;

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

            if !read_frame(&mut reader, &mut buffer, frame_index)? {
                break;
            }

            if clock.should_drop_frame(frame_index) {
                frame_index += 1;
                continue;
            }

            let current_size = size().unwrap_or((80, 60));
            if current_size != last_size {
                let _ = execute!(stdout, Clear(ClearType::All));
                last_size = current_size;
            }

            let output = renderer.render(&buffer, current_size.0, current_size.1);

            execute!(stdout, MoveTo(0, 0))?;
            print!("{output}");
            stdout.flush()?;

            frame_index += 1;
            clock.sleep_until_next_frame(frame_index, interrupted);
        }

        Ok(())
    })();

    if let Some(warning) = audio_warning {
        eprintln!("{warning}");
    }

    play_result
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
    #[test]
    fn test_playback_clock_target_time() {
        let clock = PlaybackClock::new(30.0);
        assert_eq!(clock.target_time(0), Duration::ZERO);
        assert_eq!(clock.target_time(1), clock.frame_duration);
        assert_eq!(clock.target_time(30), clock.frame_duration.mul_f64(30.0));
    }

    #[test]
    fn test_playback_clock_normalization() {
        let clock_nan = PlaybackClock::new(f64::NAN);
        assert_eq!(clock_nan.frame_duration, Duration::from_secs_f64(1.0 / 30.0));

        let clock_neg = PlaybackClock::new(-12.0);
        assert_eq!(clock_neg.frame_duration, Duration::from_secs_f64(1.0 / 30.0));

        let clock_zero = PlaybackClock::new(0.0);
        assert_eq!(clock_zero.frame_duration, Duration::from_secs_f64(1.0 / 30.0));

        let clock_valid = PlaybackClock::new(60.0);
        assert_eq!(clock_valid.frame_duration, Duration::from_secs_f64(1.0 / 60.0));

        let clock_huge = PlaybackClock::new(1e12);
        assert_eq!(clock_huge.frame_duration, Duration::from_secs_f64(1.0 / 30.0));
    }
    #[test]
    fn test_playback_clock_should_drop_frame() {
        let mut clock = PlaybackClock::new(30.0);
        assert!(!clock.should_drop_frame(0));

        clock.start_time = Instant::now() - Duration::from_millis(50);
        assert!(clock.should_drop_frame(0));
        assert!(!clock.should_drop_frame(1));
    }

    #[test]
    fn test_playback_clock_sleep_until_next_frame() {
        let clock = PlaybackClock::new(100.0);
        let interrupted = AtomicBool::new(false);
        clock.sleep_until_next_frame(0, &interrupted);
    }

    #[test]
    fn test_read_frame_truncated_returns_error() {
        let mut buffer = vec![0u8; (WIDTH * HEIGHT) as usize];
        let mut cursor = std::io::Cursor::new(b"0123456789".to_vec());
        let err = read_frame(&mut cursor, &mut buffer, 42).unwrap_err();
        assert!(
            err.to_string().contains("Corrupt or truncated frame 42"),
            "Error message must specify corrupt or truncated frame with index"
        );
    }

    #[test]
    fn test_read_frame_clean_eof() {
        let mut buffer = vec![0u8; (WIDTH * HEIGHT) as usize];
        let mut empty = std::io::Cursor::new(Vec::new());
        let has_frame = read_frame(&mut empty, &mut buffer, 0).unwrap();
        assert!(!has_frame, "Empty reader must return clean EOF");
    }

    #[test]
    fn test_read_frame_success() {
        let mut buffer = vec![0u8; (WIDTH * HEIGHT) as usize];
        let data = vec![b'X'; (WIDTH * HEIGHT) as usize];
        let mut cursor = std::io::Cursor::new(data);
        let has_frame = read_frame(&mut cursor, &mut buffer, 0).unwrap();
        assert!(has_frame, "Full frame reader must return true");
        assert_eq!(buffer[0], b'X');
        assert_eq!(buffer[buffer.len() - 1], b'X');
    }
    #[test]
    fn test_read_frame_empty_buffer_returns_error() {
        let mut buffer = [];
        let mut cursor = std::io::Cursor::new(b"X".to_vec());
        let result = read_frame(&mut cursor, &mut buffer, 0);
        assert!(
            result.is_err(),
            "Empty buffer must return an error without panicking"
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "Frame buffer cannot be empty"
        );
    }
}
