use std::{
    io::{self, Stdout},
    panic,
    sync::Once,
};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use miette::IntoDiagnostic;
use ratatui::{backend::CrosstermBackend, Terminal};

use super::{app::TuiApp, render};

static PANIC_HOOK: Once = Once::new();

pub(super) struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    pub(super) fn enter() -> miette::Result<Self> {
        install_panic_hook();
        enable_raw_mode().into_diagnostic()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).into_diagnostic()?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).into_diagnostic()?;
        terminal.clear().into_diagnostic()?;
        Ok(Self { terminal })
    }

    pub(super) fn draw(&mut self, app: &TuiApp) -> miette::Result<()> {
        self.terminal
            .draw(|frame| render::render(frame, app))
            .into_diagnostic()?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let original = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            original(info);
        }));
    });
}
