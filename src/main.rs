mod builder;
mod cli;
mod player;
mod render;
mod terminal;

use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

fn init_signal_handler() {
    let _ = ctrlc::set_handler(|| {
        if INTERRUPTED.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
    });
}

fn main() {
    init_signal_handler();
    terminal::install_panic_hook();
    let cli = cli::Cli::parse();

    let result = match &cli.command {
        cli::Commands::Build { frames_dir, output } => builder::build_frames(frames_dir, output),
        cli::Commands::Play { input, audio, fps } => player::play(input, audio, *fps),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        if INTERRUPTED.load(Ordering::Relaxed) {
            std::process::exit(130);
        }
        std::process::exit(1);
    }
}
