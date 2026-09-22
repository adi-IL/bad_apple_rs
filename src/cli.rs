use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
        #[arg(short, long, default_value = crate::player::DEFAULT_AUDIO_PATH)]
        audio: String,
        #[arg(long, default_value_t = crate::player::DEFAULT_FPS, allow_negative_numbers = true)]
        fps: f64,
    },
}
