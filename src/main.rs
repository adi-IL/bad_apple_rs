use clap::{Parser, Subcommand};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
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
    },
}

const WIDTH: u32 = 80;
const HEIGHT: u32 = 60;
const ASCII_CHARS: &[u8] = b" .:-=+*#%@";

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Build { frames_dir, output } => {
            build_frames(frames_dir, output);
        }
        Commands::Play { input, audio } => {
            play(input, audio);
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
                // Map 0-255 to 0-(ASCII_CHARS.len()-1)
                let idx = (pixel as usize * (ASCII_CHARS.len() - 1)) / 255;
                frame_data.push(ASCII_CHARS[idx]);
            }
        }
        out_file.write_all(&frame_data).unwrap();
        i += 1;
    }
}

fn play(input: &str, audio_path: &str) {
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

    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All)).unwrap();

    let frame_size = (WIDTH * HEIGHT) as usize;
    let mut buffer = vec![0u8; frame_size];

    let fps = 30.0;
    let frame_duration = Duration::from_secs_f64(1.0 / fps);
    let start_time = Instant::now();
    let mut frame_count = 0;

    thread::sleep(Duration::from_millis(500));

    loop {
        match reader.read_exact(&mut buffer) {
            Ok(_) => {
                let (term_width, term_height) = size().unwrap_or((80, 60));

                let pad_x = if term_width > WIDTH as u16 {
                    (term_width - WIDTH as u16) / 2
                } else {
                    0
                };
                let pad_y = if term_height > HEIGHT as u16 {
                    (term_height - HEIGHT as u16) / 2
                } else {
                    0
                };

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
                    let line = std::str::from_utf8(&buffer[start..end]).unwrap();
                    output.push_str(line);
                    output.push('\n');
                }

                execute!(stdout, MoveTo(0, 0)).unwrap();
                print!("{}", output);
                std::io::stdout().flush().unwrap();

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

    execute!(stdout, Show, LeaveAlternateScreen).unwrap();
    println!("Finished playing.");
}
