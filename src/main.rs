mod format;
mod graphics;
mod interpreter;
mod lexer;
mod parser;

use arboard::Clipboard;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, Once};
use std::thread;
use std::time::{Duration, Instant};
use tui_textarea::{CursorMove, TextArea};

use crate::interpreter::ConsoleMsg;

const APP_NAME: &str = "QBasic Studio";
const DEFAULT_FILE_NAME: &str = "main.bas";
const DEFAULT_SOURCE: &str = "' Welcome to QBasic Studio
RANDOMIZE TIMER
DATA \"Ada\", \"Grace\", \"Katherine\"

PRINT \"QBasic Studio is ready.\"
FOR I = 1 TO 3
    READ NAME$
    PRINT \"HELLO, \"; NAME$
NEXT I
";
const CURSOR_LINE_BG: Color = Color::Rgb(20, 28, 34);
/// A terminal that exits this quickly after launch most likely handed the
/// program to an already-running terminal server and detached.
const DETACH_THRESHOLD: Duration = Duration::from_millis(1500);

enum CliCommand {
    Ide(Option<PathBuf>),
    Run(PathBuf),
    RunWindow(PathBuf),
    Check(PathBuf),
    Help,
    Version,
}

#[derive(Clone, Copy)]
enum PendingAction {
    Quit,
    New,
    Open,
}

enum AppState {
    Editing,
    PromptingSave(String),
    PromptingLoad(String),
    Confirm(String, PendingAction),
}

struct RunningProgram {
    child: Child,
    source_path: PathBuf,
    started: Instant,
}

#[derive(Default)]
struct EditorView {
    top_row: usize,
    left_col: usize,
}

struct EditorRenderMeta<'a> {
    current_file: &'a Path,
    dirty: bool,
    is_running: bool,
    show_cursor: bool,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<ExitCode, Box<dyn Error>> {
    match parse_cli(env::args().skip(1).collect())? {
        CliCommand::Ide(path) => {
            run_ide(path)?;
            Ok(ExitCode::SUCCESS)
        }
        CliCommand::Run(path) => Ok(report(run_file(&path))),
        CliCommand::RunWindow(path) => {
            let code = report(run_file(&path));
            pause_before_close();
            Ok(code)
        }
        CliCommand::Check(path) => match check_file(&path) {
            Ok(()) => {
                println!("{}: syntax ok", path.display());
                Ok(ExitCode::SUCCESS)
            }
            Err(err) => {
                eprintln!("{}", err);
                Ok(ExitCode::FAILURE)
            }
        },
        CliCommand::Help => {
            println!("{}", help_text());
            Ok(ExitCode::SUCCESS)
        }
        CliCommand::Version => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", err);
            ExitCode::FAILURE
        }
    }
}

fn parse_cli(args: Vec<String>) -> Result<CliCommand, String> {
    match args.as_slice() {
        [] => Ok(CliCommand::Ide(None)),
        [flag] if flag == "-h" || flag == "--help" || flag == "help" => Ok(CliCommand::Help),
        [flag] if flag == "-V" || flag == "--version" || flag == "version" => {
            Ok(CliCommand::Version)
        }
        [cmd, path] if cmd == "run" => Ok(CliCommand::Run(PathBuf::from(path))),
        [cmd, path] if cmd == "run-window" => Ok(CliCommand::RunWindow(PathBuf::from(path))),
        [cmd, path] if cmd == "check" => Ok(CliCommand::Check(PathBuf::from(path))),
        [cmd] if cmd == "edit" || cmd == "ide" => Ok(CliCommand::Ide(None)),
        [cmd, path] if cmd == "edit" || cmd == "ide" => {
            Ok(CliCommand::Ide(Some(PathBuf::from(path))))
        }
        [path] if !path.starts_with('-') => Ok(CliCommand::Run(PathBuf::from(path))),
        _ => Err(format!("invalid arguments\n\n{}", help_text())),
    }
}

fn help_text() -> String {
    format!(
        "{name} {version}

USAGE:
    qbasic_interpreter                 Launch the terminal IDE
    qbasic_interpreter <file.bas>      Run a BASIC program
    qbasic_interpreter run <file.bas>  Run a BASIC program
    qbasic_interpreter check <file>    Parse and validate a program
    qbasic_interpreter edit [file]     Open the IDE, optionally with a file

SHORTCUTS:
    F5/Ctrl+R run   F6 stop   F2/Ctrl+S save   F12 save as
    F3/Ctrl+O open  Ctrl+N new   Esc/Ctrl+Q quit

ENVIRONMENT:
    QBASIC_STUDIO_RUN=inline       Run programs in the IDE's own terminal
    QBASIC_STUDIO_TERMINAL=\"cmd\"   Terminal command used to open run windows,
                                   e.g. \"wezterm start --\"",
        name = APP_NAME,
        version = env!("CARGO_PKG_VERSION")
    )
}

/// Restores the terminal if anything panics while it is in raw mode.
fn install_terminal_panic_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
            previous(info);
        }));
    });
}

// ---- IDE ---------------------------------------------------------------------

type Term = Terminal<CrosstermBackend<io::Stdout>>;

fn run_ide(open_file: Option<PathBuf>) -> Result<(), Box<dyn Error>> {
    install_terminal_panic_hook();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_ide_loop(&mut terminal, open_file);

    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    terminal.show_cursor()?;

    result
}

struct Ide {
    textarea: TextArea<'static>,
    current_file: PathBuf,
    dirty: bool,
    status_msg: String,
    status_color: Color,
    view: EditorView,
    state: AppState,
    running: Option<RunningProgram>,
    /// Temporary sources of runs whose terminal detached; removed on exit.
    detached_sources: Vec<PathBuf>,
}

impl Ide {
    fn set_status(&mut self, msg: impl Into<String>, color: Color) {
        self.status_msg = msg.into();
        self.status_color = color;
    }

    fn load_new(&mut self, path: PathBuf, source: &str, dirty: bool) {
        self.textarea = textarea_from_source(source);
        self.view = EditorView::default();
        self.current_file = path;
        self.dirty = dirty;
    }

    fn save_to(&mut self, path: PathBuf) {
        match save_editor(&path, &self.textarea) {
            Ok(()) => {
                self.current_file = path;
                self.dirty = false;
                let msg = format!("Saved {}", display_path(&self.current_file));
                self.set_status(msg, Color::Green);
            }
            Err(err) => self.set_status(format!("Save failed: {}", err), Color::Red),
        }
    }

    fn request(&mut self, action: PendingAction) -> bool {
        let needs_confirm =
            self.dirty || (matches!(action, PendingAction::Quit) && self.running.is_some());
        if !needs_confirm {
            return self.perform(action);
        }
        let message = match action {
            PendingAction::Quit => quit_reason(self.dirty, self.running.is_some()),
            PendingAction::New => {
                "Discard unsaved changes and start a new program? y/n".to_string()
            }
            PendingAction::Open => "Discard unsaved changes and open another file? y/n".to_string(),
        };
        self.state = AppState::Confirm(message, action);
        false
    }

    /// Carries out an action; returns `true` when the IDE should exit.
    fn perform(&mut self, action: PendingAction) -> bool {
        self.state = AppState::Editing;
        match action {
            PendingAction::Quit => {
                stop_running_program(&mut self.running);
                return true;
            }
            PendingAction::New => {
                self.load_new(PathBuf::from(DEFAULT_FILE_NAME), "", false);
                self.set_status("New program", Color::Green);
            }
            PendingAction::Open => {
                self.state =
                    AppState::PromptingLoad(self.current_file.to_string_lossy().to_string());
            }
        }
        false
    }

    fn cleanup(&mut self) {
        stop_running_program(&mut self.running);
        for p in self.detached_sources.drain(..) {
            let _ = fs::remove_file(p);
        }
    }
}

fn run_ide_loop(terminal: &mut Term, open_file: Option<PathBuf>) -> Result<(), Box<dyn Error>> {
    let mut clipboard = Clipboard::new().ok();
    let current_file = open_file.unwrap_or_else(|| PathBuf::from(DEFAULT_FILE_NAME));
    let mut ide = Ide {
        textarea: textarea_from_source(""),
        current_file: current_file.clone(),
        dirty: false,
        status_msg: String::from("Ready"),
        status_color: Color::Green,
        view: EditorView::default(),
        state: AppState::Editing,
        running: None,
        detached_sources: Vec::new(),
    };
    if current_file.exists() {
        match fs::read_to_string(&current_file) {
            Ok(content) => {
                ide.load_new(current_file.clone(), &content, false);
                ide.set_status(
                    format!("Loaded {}", display_path(&current_file)),
                    Color::Green,
                );
            }
            Err(err) => {
                ide.load_new(current_file.clone(), DEFAULT_SOURCE, false);
                ide.set_status(
                    format!("Could not load {}: {}", display_path(&current_file), err),
                    Color::Red,
                );
            }
        }
    } else {
        ide.load_new(current_file, DEFAULT_SOURCE, true);
    }

    let mut needs_redraw = true;
    loop {
        if needs_redraw {
            draw_ide(terminal, &mut ide)?;
            needs_redraw = false;
        }
        if poll_running_program(&mut ide) {
            needs_redraw = true;
        }
        let timeout = if ide.running.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(250)
        };
        if !event::poll(timeout)? {
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => key,
            Event::Resize(_, _) => {
                needs_redraw = true;
                continue;
            }
            _ => continue,
        };
        needs_redraw = true;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let is_save = matches!(ide.state, AppState::PromptingSave(_));
        match &mut ide.state {
            AppState::PromptingSave(input) | AppState::PromptingLoad(input) => match key.code {
                KeyCode::Enter => {
                    let input = match std::mem::replace(&mut ide.state, AppState::Editing) {
                        AppState::PromptingSave(s) | AppState::PromptingLoad(s) => s,
                        _ => String::new(),
                    };
                    let target = prompt_target_path(&input, &ide.current_file);
                    if is_save {
                        ide.save_to(target);
                    } else {
                        match fs::read_to_string(&target) {
                            Ok(content) => {
                                ide.load_new(target, &content, false);
                                let msg = format!("Loaded {}", display_path(&ide.current_file));
                                ide.set_status(msg, Color::Green);
                            }
                            Err(err) => ide.set_status(format!("Load failed: {}", err), Color::Red),
                        }
                    }
                }
                KeyCode::Esc => ide.state = AppState::Editing,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) if !ctrl => input.push(c),
                _ => {}
            },
            AppState::Confirm(_, action) => {
                let action = *action;
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        if ide.perform(action) {
                            break;
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        ide.state = AppState::Editing;
                        ide.set_status("Cancelled", Color::Green);
                    }
                    _ => {}
                }
            }
            AppState::Editing => match key.code {
                KeyCode::Esc => {
                    if ide.request(PendingAction::Quit) {
                        break;
                    }
                }
                KeyCode::Char('q') if ctrl => {
                    if ide.request(PendingAction::Quit) {
                        break;
                    }
                }
                KeyCode::F(5) => run_from_ide(terminal, &mut ide)?,
                KeyCode::Char('r') if ctrl => run_from_ide(terminal, &mut ide)?,
                KeyCode::F(6) => {
                    if ide.running.is_some() {
                        stop_running_program(&mut ide.running);
                        ide.set_status("Program window closed", Color::Yellow);
                    }
                }
                KeyCode::F(2) => ide.save_to(ide.current_file.clone()),
                KeyCode::Char('s') if ctrl => ide.save_to(ide.current_file.clone()),
                KeyCode::F(12) => {
                    ide.state =
                        AppState::PromptingSave(ide.current_file.to_string_lossy().to_string());
                }
                KeyCode::F(3) => {
                    ide.request(PendingAction::Open);
                }
                KeyCode::Char('o') if ctrl => {
                    ide.request(PendingAction::Open);
                }
                KeyCode::Char('n') if ctrl => {
                    ide.request(PendingAction::New);
                }
                KeyCode::Char('c' | 'C') if ctrl => {
                    ide.textarea.copy();
                    if let Some(cb) = clipboard.as_mut() {
                        let _ = cb.set_text(ide.textarea.yank_text());
                        ide.set_status("Copied to clipboard", Color::Green);
                    }
                }
                KeyCode::Char('x' | 'X') if ctrl => {
                    if ide.textarea.cut() {
                        ide.dirty = true;
                        if let Some(cb) = clipboard.as_mut() {
                            let _ = cb.set_text(ide.textarea.yank_text());
                        }
                        ide.set_status("Cut to clipboard", Color::Green);
                    }
                }
                KeyCode::Char('v' | 'V') if ctrl => {
                    if let Some(text) = clipboard.as_mut().and_then(|cb| cb.get_text().ok()) {
                        ide.textarea.set_yank_text(text);
                    }
                    if ide.textarea.paste() {
                        ide.dirty = true;
                        ide.set_status("Pasted", Color::Green);
                    }
                }
                _ => {
                    if ide.textarea.input(key) {
                        ide.dirty = true;
                        ide.set_status("Editing", Color::Green);
                    }
                }
            },
        }
    }
    ide.cleanup();
    Ok(())
}

fn draw_ide(terminal: &mut Term, ide: &mut Ide) -> io::Result<()> {
    let is_running = ide.running.is_some();
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(3),
            ])
            .split(f.area());

        let header = Paragraph::new(header_line(&ide.current_file, ide.dirty, is_running)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", APP_NAME)),
        );
        f.render_widget(header, chunks[0]);

        render_basic_editor(
            f,
            chunks[1],
            &ide.textarea,
            EditorRenderMeta {
                current_file: &ide.current_file,
                dirty: ide.dirty,
                is_running,
                show_cursor: matches!(ide.state, AppState::Editing),
            },
            &mut ide.view,
        );

        let footer = Paragraph::new(footer_text(&ide.state, &ide.status_msg))
            .style(
                Style::default()
                    .fg(footer_color(&ide.state, ide.status_color))
                    .bg(Color::Black),
            )
            .block(Block::default().borders(Borders::ALL).title(" Status "));
        f.render_widget(footer, chunks[2]);
    })?;
    Ok(())
}

fn textarea_from_source(source: &str) -> TextArea<'static> {
    let lines: Vec<String> = if source.is_empty() {
        vec![String::new()]
    } else {
        source.lines().map(|line| line.to_string()).collect()
    };
    let mut textarea = TextArea::from(lines);
    textarea.set_cursor_line_style(Style::default().bg(CURSOR_LINE_BG));
    textarea.set_cursor_style(Style::default().fg(Color::Black).bg(Color::White));
    textarea
}

fn render_basic_editor(
    f: &mut Frame<'_>,
    area: Rect,
    textarea: &TextArea<'static>,
    meta: EditorRenderMeta<'_>,
    view: &mut EditorView,
) {
    let marker = if meta.dirty { " *" } else { "" };
    let run_marker = if meta.is_running { " [running]" } else { "" };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Editor - {}{}{} ",
            display_path(meta.current_file),
            marker,
            run_marker
        ))
        .style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = textarea.lines();
    let (cursor_row, cursor_col) = textarea.cursor();
    let height = inner.height as usize;
    if height == 0 {
        return;
    }
    let gutter_width = basic_gutter_width(lines.len());
    let text_width = (inner.width as usize).saturating_sub(gutter_width);

    if cursor_row < view.top_row {
        view.top_row = cursor_row;
    } else if cursor_row >= view.top_row.saturating_add(height) {
        view.top_row = cursor_row + 1 - height;
    }
    if text_width > 0 {
        if cursor_col < view.left_col {
            view.left_col = cursor_col;
        } else if cursor_col >= view.left_col.saturating_add(text_width) {
            view.left_col = cursor_col + 1 - text_width;
        }
    }

    let selection = textarea.selection_range();
    let bottom = lines.len().min(view.top_row + height);
    let mut rendered = Vec::with_capacity(height);
    for (row, line) in lines.iter().enumerate().take(bottom).skip(view.top_row) {
        let is_cursor_row = row == cursor_row;
        let line_bg = if is_cursor_row {
            Style::default().bg(CURSOR_LINE_BG)
        } else {
            Style::default()
        };
        let mut spans = vec![Span::styled(
            format!(
                "{:>width$} ",
                display_basic_line_number(row, line),
                width = gutter_width.saturating_sub(1)
            ),
            line_bg.fg(Color::DarkGray),
        )];
        let selected = selection.and_then(|((sr, sc), (er, ec))| {
            if row < sr || row > er {
                return None;
            }
            let start = if row == sr { sc } else { 0 };
            let end = if row == er { ec } else { usize::MAX };
            Some((start, end))
        });
        spans.extend(styled_line_spans(
            line,
            view.left_col,
            text_width,
            line_bg,
            selected,
        ));
        rendered.push(Line::from(spans));
    }
    while rendered.len() < height {
        rendered.push(Line::from(Span::raw("")));
    }

    f.render_widget(Paragraph::new(rendered), inner);

    if meta.show_cursor
        && cursor_row >= view.top_row
        && cursor_row < view.top_row + height
        && cursor_col >= view.left_col
        && text_width > 0
    {
        let x = inner.x + gutter_width as u16 + (cursor_col - view.left_col) as u16;
        let y = inner.y + (cursor_row - view.top_row) as u16;
        if x < inner.x.saturating_add(inner.width) && y < inner.y.saturating_add(inner.height) {
            f.set_cursor_position((x, y));
        }
    }
}

fn basic_gutter_width(line_count: usize) -> usize {
    let max_auto = auto_basic_line_number(line_count.saturating_sub(1));
    max_auto.to_string().len().max(4) + 1
}

fn auto_basic_line_number(row: usize) -> u32 {
    ((row as u32).saturating_add(1)).saturating_mul(10)
}

fn display_basic_line_number(row: usize, line: &str) -> String {
    explicit_basic_line_number(line).unwrap_or_else(|| auto_basic_line_number(row).to_string())
}

fn explicit_basic_line_number(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let len = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if len == 0 {
        return None;
    }
    Some(trimmed.chars().take(len).collect())
}

fn source_with_auto_line_numbers(lines: &[String]) -> String {
    let mut source = String::new();
    for (row, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || explicit_basic_line_number(line).is_some() {
            source.push_str(line);
        } else {
            source.push_str(&format!("{} {}", auto_basic_line_number(row), line));
        }
        source.push('\n');
    }
    source
}

/// Syntax colour for every character of a line.
fn highlight_styles(line: &str) -> Vec<Style> {
    let chars: Vec<char> = line.chars().collect();
    let mut styles = vec![Style::default(); chars.len()];
    let comment = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let style;
        if c == '\'' {
            styles[i..].fill(comment);
            break;
        } else if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            style = Style::default().fg(Color::Green);
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            style = Style::default().fg(Color::Magenta);
        } else if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            if i < chars.len() && matches!(chars[i], '$' | '%' | '!' | '#' | '&') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect::<String>().to_uppercase();
            if word == "REM" {
                styles[start..].fill(comment);
                break;
            }
            style = if crate::lexer::is_keyword(&word) {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if crate::parser::is_builtin(&word) {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
        } else {
            i += 1;
            style = if c.is_whitespace() {
                Style::default()
            } else {
                Style::default().fg(Color::Blue)
            };
        }
        styles[start..i].fill(style);
    }
    styles
}

/// Builds the visible spans of a line: syntax colours, the cursor-line
/// background and any selected columns, clipped to the viewport.
fn styled_line_spans(
    line: &str,
    left_col: usize,
    text_width: usize,
    line_bg: Style,
    selected: Option<(usize, usize)>,
) -> Vec<Span<'static>> {
    let chars: Vec<char> = line.chars().collect();
    let styles = highlight_styles(line);
    let selected_style = Style::default().bg(Color::Blue).fg(Color::White);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_style = Style::default();
    let end = (left_col + text_width).min(chars.len() + 1);
    #[allow(clippy::needless_range_loop)]
    for col in left_col..end {
        let is_sel = selected.is_some_and(|(s, e)| col >= s && col < e);
        let (ch, base) = match chars.get(col) {
            Some(ch) => (*ch, line_bg.patch(styles[col])),
            // Show a selected line break as one highlighted cell.
            None if is_sel => (' ', line_bg),
            None => break,
        };
        let style = if is_sel { selected_style } else { base };
        if style != run_style && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
        }
        run_style = style;
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, run_style));
    }
    spans
}

fn header_line(path: &Path, dirty: bool, is_running: bool) -> Line<'static> {
    let state = if is_running { "RUNNING" } else { "READY" };
    let saved = if dirty { "modified" } else { "saved" };
    Line::from(vec![
        Span::styled(
            format!(" {} ", display_path(path)),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{} ", saved),
            Style::default().fg(if dirty { Color::Yellow } else { Color::Green }),
        ),
        Span::raw(" "),
        Span::styled(state, Style::default().fg(Color::Cyan)),
    ])
}

fn footer_text(app_state: &AppState, status_msg: &str) -> String {
    match app_state {
        AppState::PromptingSave(input) => format!("Save as: {}_", input),
        AppState::PromptingLoad(input) => format!("Open file: {}_", input),
        AppState::Confirm(message, _) => message.clone(),
        AppState::Editing => format!(
            "{} | F5 Run  F6 Stop  F2 Save  F3 Open  Esc Quit",
            status_msg
        ),
    }
}

fn footer_color(app_state: &AppState, status_color: Color) -> Color {
    match app_state {
        AppState::Editing => status_color,
        _ => Color::Yellow,
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn prompt_target_path(input: &str, current_file: &Path) -> PathBuf {
    if input.trim().is_empty() {
        current_file.to_path_buf()
    } else {
        PathBuf::from(input.trim())
    }
}

/// Saves atomically: the text is written to a temporary file next to the
/// target, flushed to disk and then renamed over it, so a crash or full disk
/// never leaves a half-written program behind.
fn save_editor(path: &Path, textarea: &TextArea<'static>) -> Result<(), String> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    {
        let mut w = io::BufWriter::new(tmp.as_file_mut());
        for line in textarea.lines() {
            w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            w.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        w.flush().map_err(|e| e.to_string())?;
    }
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    if let Ok(meta) = fs::metadata(path) {
        let _ = tmp.as_file().set_permissions(meta.permissions());
    }
    tmp.persist(path).map_err(|e| e.error.to_string())?;
    Ok(())
}

fn quit_reason(dirty: bool, is_running: bool) -> String {
    match (dirty, is_running) {
        (true, true) => String::from("Program is running and has unsaved changes. Quit? y/n"),
        (true, false) => String::from("Unsaved changes will be lost. Quit? y/n"),
        (false, true) => String::from("Program is running. Stop and quit? y/n"),
        (false, false) => String::from("Quit? y/n"),
    }
}

/// Moves the editor cursor to the line named in a "Line N: ..." message.
fn jump_to_error_line(textarea: &mut TextArea<'static>, msg: &str) {
    let line = msg
        .strip_prefix("Line ")
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<u16>().ok());
    if let Some(n) = line {
        textarea.move_cursor(CursorMove::Jump(n.saturating_sub(1), 0));
    }
}

fn run_from_ide(terminal: &mut Term, ide: &mut Ide) -> Result<(), Box<dyn Error>> {
    if ide.running.is_some() {
        ide.set_status("Program is already running (F6 stops it)", Color::Yellow);
        return Ok(());
    }
    let source = source_with_auto_line_numbers(ide.textarea.lines());
    if let Err(err) = lexer::tokenize(&source).and_then(parser::parse) {
        jump_to_error_line(&mut ide.textarea, &err);
        ide.set_status(format!("Syntax error: {}", err), Color::Red);
        return Ok(());
    }
    let source_path = match write_run_source(&source) {
        Ok(path) => path,
        Err(err) => {
            ide.set_status(format!("Run setup failed: {}", err), Color::Red);
            return Ok(());
        }
    };
    let run_cwd = run_working_directory(&ide.current_file);
    match spawn_program_window(&source_path, &run_cwd) {
        Some(Ok(child)) => {
            ide.running = Some(RunningProgram {
                child,
                source_path,
                started: Instant::now(),
            });
            ide.set_status(
                "Running in a separate window. Close it or press F6 to stop.",
                Color::Cyan,
            );
        }
        Some(Err(err)) => {
            let _ = fs::remove_file(&source_path);
            ide.set_status(format!("Run launch failed: {}", err), Color::Red);
        }
        None => {
            run_inline(terminal, ide, &source_path, &run_cwd)?;
            let _ = fs::remove_file(&source_path);
        }
    }
    Ok(())
}

/// Runs the program in the IDE's own terminal: the editor screen is put
/// aside, the program runs full-screen, and the editor comes back afterwards.
/// The terminal stays in raw mode, so Ctrl+C reaches the program (which stops
/// itself) instead of killing the IDE.
fn run_inline(
    terminal: &mut Term,
    ide: &mut Ide,
    source_path: &Path,
    run_cwd: &Path,
) -> Result<(), Box<dyn Error>> {
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    print!(
        "\r\n--- Running {} (Ctrl+C stops) ---\r\n",
        display_path(&ide.current_file)
    );
    io::stdout().flush()?;
    let status = env::current_exe().and_then(|exe| {
        Command::new(exe)
            .arg("run-window")
            .arg(source_path)
            .current_dir(run_cwd)
            .status()
    });
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    match status {
        Ok(s) if s.success() => ide.set_status("Program finished", Color::Green),
        Ok(s) => ide.set_status(format!("Program exited with {}", s), Color::Red),
        Err(e) => ide.set_status(format!("Run launch failed: {}", e), Color::Red),
    }
    Ok(())
}

fn write_run_source(source: &str) -> Result<PathBuf, String> {
    let mut file = tempfile::Builder::new()
        .prefix("qbasic_studio_run_")
        .suffix(".bas")
        .tempfile()
        .map_err(|e| e.to_string())?;
    file.write_all(source.as_bytes())
        .map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    let (_, path) = file.keep().map_err(|e| e.to_string())?;
    Ok(path)
}

fn run_working_directory(current_file: &Path) -> PathBuf {
    current_file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn force_inline_run() -> bool {
    env::var("QBASIC_STUDIO_RUN").is_ok_and(|v| v.eq_ignore_ascii_case("inline"))
}

/// Launches the program in a new window. `None` means no window can be
/// opened here and the program should run inline instead.
#[cfg(windows)]
fn spawn_program_window(source_path: &Path, run_cwd: &Path) -> Option<Result<Child, String>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    if force_inline_run() {
        return None;
    }
    let exe = match env::current_exe() {
        Ok(exe) => exe,
        Err(e) => return Some(Err(e.to_string())),
    };
    Some(
        Command::new(exe)
            .arg("run-window")
            .arg(source_path)
            .current_dir(run_cwd)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|err| err.to_string()),
    )
}

#[cfg(not(windows))]
fn spawn_program_window(source_path: &Path, run_cwd: &Path) -> Option<Result<Child, String>> {
    if force_inline_run() {
        return None;
    }
    let exe = match env::current_exe() {
        Ok(exe) => exe,
        Err(e) => return Some(Err(e.to_string())),
    };
    let mut candidates: Vec<Vec<String>> = Vec::new();
    if let Ok(custom) = env::var("QBASIC_STUDIO_TERMINAL") {
        candidates.push(custom.split_whitespace().map(String::from).collect());
    }
    let has_display = env::var_os("DISPLAY").is_some() || env::var_os("WAYLAND_DISPLAY").is_some();
    if cfg!(target_os = "linux") && has_display {
        for t in [
            "x-terminal-emulator -e",
            "gnome-terminal --wait --",
            "konsole --nofork -e",
            "xfce4-terminal --disable-server -x",
            "alacritty -e",
            "kitty --",
            "foot",
            "xterm -e",
        ] {
            candidates.push(t.split_whitespace().map(String::from).collect());
        }
    }
    for cmd in candidates {
        let Some((program, args)) = cmd.split_first() else {
            continue;
        };
        if let Ok(child) = Command::new(program)
            .args(args)
            .arg(&exe)
            .arg("run-window")
            .arg(source_path)
            .current_dir(run_cwd)
            .spawn()
        {
            return Some(Ok(child));
        }
    }
    None
}

fn poll_running_program(ide: &mut Ide) -> bool {
    let Some(program) = ide.running.as_mut() else {
        return false;
    };
    let status = match program.child.try_wait() {
        Ok(None) => return false,
        other => other,
    };
    let program = ide.running.take().unwrap();
    match status {
        Ok(Some(s)) if s.success() && program.started.elapsed() < DETACH_THRESHOLD => {
            // The terminal handed the program to another process; we can no
            // longer track it, so keep its source until the IDE exits.
            ide.detached_sources.push(program.source_path);
            ide.set_status(
                "Program opened in a terminal window (detached; close it there)",
                Color::Cyan,
            );
        }
        Ok(Some(s)) => {
            let _ = fs::remove_file(&program.source_path);
            if s.success() {
                ide.set_status("Program finished", Color::Green);
            } else {
                ide.set_status(format!("Program exited with {}", s), Color::Red);
            }
        }
        Err(err) => {
            let _ = fs::remove_file(&program.source_path);
            ide.set_status(format!("Program status failed: {}", err), Color::Red);
        }
        Ok(None) => {}
    }
    true
}

fn stop_running_program(running_program: &mut Option<RunningProgram>) {
    if let Some(mut program) = running_program.take() {
        let _ = program.child.kill();
        let _ = program.child.wait();
        let _ = fs::remove_file(program.source_path);
    }
}

// ---- Command-line runner -------------------------------------------------------

fn check_file(path: &Path) -> Result<(), String> {
    let src = fs::read_to_string(path)
        .map_err(|err| format!("Error reading {}: {}", path.display(), err))?;
    lexer::tokenize(&src)
        .and_then(parser::parse)
        .map(|_| ())
        .map_err(|err| format!("{}: {}", path.display(), err))
}

/// Maps a QBasic colour index (0-15) to an ANSI colour offset.
fn ansi_color(qb: u8) -> (u8, bool) {
    const MAP: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];
    (MAP[(qb % 8) as usize], qb % 16 >= 8)
}

/// The string `INKEY$` returns for a key press, following QBasic's codes
/// (extended keys are CHR$(0) + scan code).
fn inkey_code(key: &KeyEvent) -> Option<String> {
    let ext = |scan: u8| Some(format!("\0{}", scan as char));
    match key.code {
        KeyCode::Char(c)
            if key.modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii_alphabetic() =>
        {
            Some(((c.to_ascii_uppercase() as u8 - b'A' + 1) as char).to_string())
        }
        KeyCode::Char(c) => Some(c.to_string()),
        KeyCode::Enter => Some("\r".to_string()),
        KeyCode::Esc => Some("\x1B".to_string()),
        KeyCode::Backspace => Some("\x08".to_string()),
        KeyCode::Tab => Some("\t".to_string()),
        KeyCode::Up => ext(72),
        KeyCode::Down => ext(80),
        KeyCode::Left => ext(75),
        KeyCode::Right => ext(77),
        KeyCode::Home => ext(71),
        KeyCode::End => ext(79),
        KeyCode::PageUp => ext(73),
        KeyCode::PageDown => ext(81),
        KeyCode::Insert => ext(82),
        KeyCode::Delete => ext(83),
        KeyCode::F(n @ 1..=10) => ext(58 + n),
        KeyCode::F(11) => ext(133),
        KeyCode::F(12) => ext(134),
        _ => None,
    }
}

fn push_key(keys: &Mutex<VecDeque<String>>, key: String) {
    if let Ok(mut q) = keys.lock() {
        if q.len() < 64 {
            q.push_back(key);
        }
    }
}

/// Front end for program text I/O. In raw mode it also reads the keyboard
/// for `INKEY$`, edits `INPUT` lines itself and turns Ctrl+C into a request
/// to stop the program.
fn console_loop(
    rx: mpsc::Receiver<ConsoleMsg>,
    input_tx: mpsc::Sender<String>,
    keys: Arc<Mutex<VecDeque<String>>>,
    cancel: Arc<AtomicBool>,
    raw: bool,
) {
    let mut out = io::stdout();
    // Escape sequences only make sense on a terminal; keep piped output clean.
    let ansi = out.is_terminal();
    let mut input_tx = Some(input_tx);
    let mut line: Option<String> = None;
    let mut colored = false;
    let nl = if raw { "\r\n" } else { "\n" };
    loop {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(ConsoleMsg::Print { text, newline }) => {
                let text = if raw {
                    text.replace('\n', "\r\n")
                } else {
                    text
                };
                let _ = write!(out, "{}{}", text, if newline { nl } else { "" });
                let _ = out.flush();
            }
            Ok(ConsoleMsg::InputPrompt(prompt)) => {
                let _ = write!(out, "{}", prompt);
                let _ = out.flush();
                if raw {
                    line = Some(String::new());
                } else {
                    let mut buf = String::new();
                    match io::stdin().read_line(&mut buf) {
                        Ok(n) if n > 0 => {
                            let text = buf.trim_end_matches(['\r', '\n']).to_string();
                            // Piped input is not echoed by a terminal; show it
                            // so transcripts read like an interactive session.
                            if !io::stdin().is_terminal() {
                                let _ = writeln!(out, "{}", text);
                                let _ = out.flush();
                            }
                            if let Some(tx) = &input_tx {
                                let _ = tx.send(text);
                            }
                        }
                        // End of input: let the program see "Input past end".
                        _ => input_tx = None,
                    }
                }
            }
            Ok(ConsoleMsg::Clear) if ansi => {
                let _ = write!(out, "\x1B[2J\x1B[1;1H");
                let _ = out.flush();
            }
            Ok(ConsoleMsg::Locate { row, col }) if ansi => {
                let _ = match (row, col) {
                    (Some(r), Some(c)) => write!(out, "\x1B[{};{}H", r, c),
                    (Some(r), None) => write!(out, "\x1B[{}d", r),
                    (None, Some(c)) => write!(out, "\x1B[{}G", c),
                    (None, None) => Ok(()),
                };
                let _ = out.flush();
            }
            Ok(ConsoleMsg::Color { fg, bg }) if ansi => {
                colored = true;
                if let Some(fg) = fg {
                    let (c, bright) = ansi_color(fg % 16);
                    let _ = write!(out, "\x1B[{}m", if bright { 90 } else { 30 } + c);
                }
                if let Some(bg) = bg {
                    let (c, bright) = ansi_color(bg);
                    let _ = write!(out, "\x1B[{}m", if bright { 100 } else { 40 } + c);
                }
                let _ = out.flush();
            }
            Ok(ConsoleMsg::Clear | ConsoleMsg::Locate { .. } | ConsoleMsg::Color { .. }) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !raw {
            continue;
        }
        while event::poll(Duration::ZERO).unwrap_or(false) {
            let Ok(Event::Key(key)) = event::read() else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            if ctrl && matches!(key.code, KeyCode::Char('c' | 'C') | KeyCode::Pause) {
                cancel.store(true, Ordering::Relaxed);
                let _ = write!(out, "^C\r\n");
                let _ = out.flush();
                continue;
            }
            match line.as_mut() {
                Some(buf) => match key.code {
                    KeyCode::Enter => {
                        let _ = write!(out, "\r\n");
                        if let Some(tx) = &input_tx {
                            let _ = tx.send(std::mem::take(buf));
                        }
                        line = None;
                    }
                    KeyCode::Backspace => {
                        if buf.pop().is_some() {
                            let _ = write!(out, "\x08 \x08");
                        }
                    }
                    KeyCode::Char(c) if !ctrl => {
                        buf.push(c);
                        let _ = write!(out, "{}", c);
                    }
                    _ => {}
                },
                None => {
                    if let Some(k) = inkey_code(&key) {
                        push_key(&keys, k);
                    }
                }
            }
            let _ = out.flush();
        }
    }
    if colored {
        let _ = write!(out, "\x1B[0m");
    }
    let _ = out.flush();
}

fn minifb_inkey(key: minifb::Key, shift: bool) -> Option<String> {
    use minifb::Key as K;
    let ext = |scan: u8| Some(format!("\0{}", scan as char));
    let k = key as u32;
    if (K::A as u32..=K::Z as u32).contains(&k) {
        let c = (b'a' + (k - K::A as u32) as u8) as char;
        return Some(if shift { c.to_ascii_uppercase() } else { c }.to_string());
    }
    if (K::Key0 as u32..=K::Key9 as u32).contains(&k) {
        return Some(((b'0' + (k - K::Key0 as u32) as u8) as char).to_string());
    }
    match key {
        K::Space => Some(" ".to_string()),
        K::Enter | K::NumPadEnter => Some("\r".to_string()),
        K::Escape => Some("\x1B".to_string()),
        K::Backspace => Some("\x08".to_string()),
        K::Tab => Some("\t".to_string()),
        K::Up => ext(72),
        K::Down => ext(80),
        K::Left => ext(75),
        K::Right => ext(77),
        _ => None,
    }
}

/// The graphics window shown while a program draws.
struct GraphicsFront {
    window: Option<minifb::Window>,
    failed: bool,
}

impl GraphicsFront {
    /// Mirrors the shared frame buffer into the window and forwards key
    /// presses to `INKEY$`. Closing the window stops the program.
    fn update(
        &mut self,
        shared: &Mutex<graphics::SharedGraphics>,
        keys: &Mutex<VecDeque<String>>,
        cancel: &AtomicBool,
    ) {
        let Ok(mut g) = shared.lock() else {
            return;
        };
        if !g.active {
            self.window = None;
            return;
        }
        if self.failed {
            return;
        }
        let needs_new = self
            .window
            .as_ref()
            .is_none_or(|w| w.get_size() != (g.width, g.height));
        if needs_new {
            match minifb::Window::new(
                &g.title,
                g.width,
                g.height,
                minifb::WindowOptions::default(),
            ) {
                Ok(mut w) => {
                    w.limit_update_rate(Some(Duration::from_millis(16)));
                    self.window = Some(w);
                    g.updated = true;
                }
                Err(err) => {
                    self.failed = true;
                    let msg = format!(
                        "Graphics window could not be opened ({}); drawing commands will not be visible.",
                        err
                    );
                    eprintln!("\r\n{}\r", msg);
                    g.error = Some(msg);
                    return;
                }
            }
        }
        let Some(window) = self.window.as_mut() else {
            return;
        };
        if !window.is_open() {
            self.window = None;
            cancel.store(true, Ordering::Relaxed);
            return;
        }
        let result = if g.updated {
            g.updated = false;
            window.update_with_buffer(&g.buffer, g.width, g.height)
        } else {
            window.update();
            Ok(())
        };
        if let Err(err) = result {
            eprintln!("\r\nGraphics error: {}\r", err);
            self.window = None;
            self.failed = true;
            return;
        }
        let shift = window.is_key_down(minifb::Key::LeftShift)
            || window.is_key_down(minifb::Key::RightShift);
        for key in window.get_keys_pressed(minifb::KeyRepeat::Yes) {
            if let Some(k) = minifb_inkey(key, shift) {
                push_key(keys, k);
            }
        }
    }
}

fn run_file(path: &Path) -> Result<(), String> {
    let src = fs::read_to_string(path)
        .map_err(|err| format!("Error reading {}: {}", path.display(), err))?;
    let ast = lexer::tokenize(&src)
        .and_then(parser::parse)
        .map_err(|err| format!("{}: {}", path.display(), err))?;

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let raw = interactive && enable_raw_mode().is_ok();
    if raw {
        install_terminal_panic_hook();
    }
    let (console_tx, console_rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let graphics = Arc::new(Mutex::new(graphics::SharedGraphics::new()));
    let keys = Arc::new(Mutex::new(VecDeque::new()));

    let console = {
        let (keys, cancel) = (Arc::clone(&keys), Arc::clone(&cancel));
        thread::spawn(move || console_loop(console_rx, input_tx, keys, cancel, raw))
    };
    let handle = interpreter::spawn(
        ast,
        interpreter::Io {
            console_tx,
            input_rx,
            cancel: Arc::clone(&cancel),
            graphics: Arc::clone(&graphics),
            keys: Arc::clone(&keys),
            interactive: raw,
        },
    );

    let mut front = GraphicsFront {
        window: None,
        failed: false,
    };
    while !handle.is_finished() {
        front.update(&graphics, &keys, &cancel);
        thread::sleep(Duration::from_millis(16));
    }
    let result = handle
        .join()
        .unwrap_or_else(|_| Err("Internal error: the interpreter crashed".to_string()));
    let _ = console.join();

    // Keep a finished drawing on screen until the user dismisses it.
    if front.window.is_some() && interactive {
        if let Ok(mut g) = graphics.lock() {
            g.updated = true;
        }
        print!(
            "{}",
            if raw {
                "\r\nPress any key in the graphics window to close it.\r\n"
            } else {
                "\nPress any key in the graphics window to close it.\n"
            }
        );
        let _ = io::stdout().flush();
        if let Ok(mut q) = keys.lock() {
            q.clear();
        }
        let closed = AtomicBool::new(false);
        while front.window.is_some() && !closed.load(Ordering::Relaxed) {
            front.update(&graphics, &keys, &closed);
            if keys.lock().map(|q| !q.is_empty()).unwrap_or(false) {
                break;
            }
            thread::sleep(Duration::from_millis(16));
        }
    }
    if raw {
        let _ = disable_raw_mode();
    }
    result.map_err(|err| format!("{}: {}", path.display(), err))
}

/// Keeps a run window open after the program ends so its output can be read.
fn pause_before_close() {
    println!();
    if io::stdin().is_terminal() && enable_raw_mode().is_ok() {
        print!("Press any key to close this window.\r\n");
        let _ = io::stdout().flush();
        loop {
            match event::read() {
                Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = disable_raw_mode();
    } else {
        println!("Program window can now be closed. Press Enter to close it.");
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_source_gets_virtual_basic_line_numbers() {
        let lines = vec![
            "PRINT \"HELLO\"".to_string(),
            "GOTO 10".to_string(),
            "100 PRINT \"EXPLICIT\"".to_string(),
        ];
        assert_eq!(
            source_with_auto_line_numbers(&lines),
            "10 PRINT \"HELLO\"\n20 GOTO 10\n100 PRINT \"EXPLICIT\"\n"
        );
    }

    #[test]
    fn default_source_with_virtual_line_numbers_parses() {
        let lines: Vec<String> = DEFAULT_SOURCE.lines().map(String::from).collect();
        let source = source_with_auto_line_numbers(&lines);
        parser::parse(lexer::tokenize(&source).unwrap()).unwrap();
    }

    #[test]
    fn virtual_line_numbers_parse_block_boundaries() {
        let lines = [
            "IF 1 THEN",
            "PRINT \"A\"",
            "ELSE",
            "PRINT \"B\"",
            "END IF",
            "SELECT CASE 2",
            "CASE 1",
            "PRINT \"ONE\"",
            "CASE ELSE",
            "PRINT \"OTHER\"",
            "END SELECT",
            "WHILE 0",
            "WEND",
            "DO",
            "LOOP",
            "TYPE PAIR",
            "LEFT AS INTEGER",
            "RIGHT AS INTEGER",
            "END TYPE",
            "start:",
            "FOR I = 1 TO 2: GOSUB 230: NEXT",
            "END",
            "PRINT I: RETURN",
            "SUB DEMO()",
            "PRINT \"DEMO\"",
            "END SUB",
            "FUNCTION VALUE()",
            "VALUE = 1",
            "END FUNCTION",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        let source = source_with_auto_line_numbers(&lines);
        parser::parse(lexer::tokenize(&source).unwrap()).unwrap();
    }

    #[test]
    fn relative_default_file_uses_process_working_directory_for_run() {
        let cwd = env::current_dir().unwrap();
        assert_eq!(run_working_directory(Path::new("main.bas")), cwd);
    }

    #[test]
    fn selection_is_highlighted_and_clipped() {
        let spans = styled_line_spans("PRINT X", 0, 80, Style::default(), Some((2, 5)));
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(text, "PRINT X");
        let selected: String = spans
            .iter()
            .filter(|s| s.style.bg == Some(Color::Blue))
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(selected, "INT");
        let clipped: String = styled_line_spans("PRINT X", 2, 3, Style::default(), None)
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(clipped, "INT");
    }

    #[test]
    fn save_is_atomic_and_ends_with_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.bas");
        fs::write(&path, "old").unwrap();
        let ta = textarea_from_source("PRINT 1\nPRINT 2");
        save_editor(&path, &ta).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "PRINT 1\nPRINT 2\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn error_line_moves_cursor() {
        let mut ta = textarea_from_source("A\nB\nC\nD");
        jump_to_error_line(&mut ta, "Line 3: FOR without NEXT");
        assert_eq!(ta.cursor(), (2, 0));
    }

    #[test]
    fn inkey_codes_match_qbasic() {
        let k = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(inkey_code(&k(KeyCode::Up)).unwrap(), "\0H");
        assert_eq!(inkey_code(&k(KeyCode::F(1))).unwrap(), "\0;");
        assert_eq!(inkey_code(&k(KeyCode::Enter)).unwrap(), "\r");
        assert_eq!(
            inkey_code(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)).unwrap(),
            "\u{1}"
        );
    }
}
