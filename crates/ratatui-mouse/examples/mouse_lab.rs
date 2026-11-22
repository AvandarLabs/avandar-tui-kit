//! Interactive laboratory for the public ratatui-mouse API.
#[path = "mouse_lab/app.rs"]
mod app;
#[path = "mouse_lab/args.rs"]
mod args;
#[path = "mouse_lab/draw.rs"]
mod draw;
#[path = "mouse_lab/log.rs"]
mod log;
#[path = "mouse_lab/session.rs"]
mod session;

use app::{App, Target};
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_mouse::{MouseOutcome, MouseReport};
use serde_json::{Value, json};
use session::Session;
use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "❌ mouse_lab: {error}\n  ⚠️ Run mouse_lab --help and retry in an interactive terminal with a writable log path."
            );
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(args) = args::parse_args(std::env::args().skip(1)).map_err(io::Error::other)? else {
        print!("{}", args::HELP);
        return Ok(());
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("an interactive stdin and stdout are required").into());
    }
    eprintln!("mouse_lab: starting terminal session and JSONL log");
    let session = Session::start(&args.log_path)?;
    if args.should_query_modes {
        session.query_modes("setup")?;
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = App::new(args.settings);
    terminal.clear()?;
    draw_frame(&mut terminal, &mut app, &session)?;
    session.ready()?;
    let result = event_loop(&mut terminal, &mut app, &session);
    let cleanup = session.finish(args.should_query_modes);
    result?;
    cleanup?;
    eprintln!("✅ mouse_lab: session restored and log flushed");
    Ok(())
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    session: &Session,
) -> io::Result<()> {
    let started = Instant::now();
    let mut previous_selection = Value::Null;
    let mut previous_deadline = None;
    while !app.should_quit {
        let now = Instant::now();
        let report = app.tick(now);
        if report.outcome != MouseOutcome::Ignored || report.is_redraw_due {
            session
                .record(json!({"kind":"tick","at_ms":now.duration_since(started).as_millis()}))?;
            observe_report(app, session, &report, &mut previous_selection)?;
            if report.is_redraw_due {
                draw_frame(terminal, app, session)?;
            }
        }
        let deadline = app.controller.next_deadline();
        if previous_deadline != Some(deadline) {
            session.record(json!({"kind":"deadline","at_ms":deadline.map(|deadline|deadline.saturating_duration_since(started).as_millis())}))?;
            previous_deadline = Some(deadline);
        }
        let wait = deadline.map_or(Duration::from_secs(60), |deadline| {
            deadline.saturating_duration_since(Instant::now())
        });
        if crossterm::event::poll(wait)? {
            handle_input(terminal, app, session, &mut previous_selection)?;
        }
    }
    Ok(())
}

fn handle_input(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    session: &Session,
    previous_selection: &mut Value,
) -> io::Result<()> {
    let event = crossterm::event::read()?;
    session.record(log::event_record(&event))?;
    let old_modes = app.parser_mode_sequence();
    let report = app.handle_event(&event, Instant::now());
    observe_report(app, session, &report, previous_selection)?;
    let modes = app.parser_mode_sequence();
    if modes != old_modes {
        session.record(json!({"kind":"sequence","label":"pane_modes","destination":"parser","bytes":modes.as_bytes()}))?;
    }
    if report.is_redraw_due || matches!(event, crossterm::event::Event::Resize(..)) {
        draw_frame(terminal, app, session)?;
    }
    Ok(())
}

fn observe_report(
    app: &App,
    session: &Session,
    report: &MouseReport<Target>,
    previous_selection: &mut Value,
) -> io::Result<()> {
    session.record(log::report_record(report))?;
    let selection = log::selection_record(app.controller.selection());
    if selection != *previous_selection {
        session.record(selection.clone())?;
        *previous_selection = selection;
    }
    Ok(())
}

fn draw_frame(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    session: &Session,
) -> io::Result<()> {
    terminal.draw(|frame| draw::draw(frame, app))?;
    session.record(log::layout_record(&app.areas))
}
