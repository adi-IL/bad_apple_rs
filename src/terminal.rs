use crossterm::{
    cursor::{Hide, Show},
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};

static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);


pub fn restore_terminal() {
    if TERMINAL_ACTIVE.swap(false, Ordering::SeqCst) {
        let mut out = stdout();
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();
        default_hook(panic_info);
    }));
}

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> Result<Self, std::io::Error> {
        let mut out = stdout();
        enable_raw_mode()?;
        if let Err(err) = execute!(out, EnterAlternateScreen, Hide, Clear(ClearType::All)) {
            let _ = disable_raw_mode();
            return Err(err);
        }
        TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restore_terminal_does_not_panic() {
        restore_terminal();
    }
}
