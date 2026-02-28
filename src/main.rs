use std::ops::Range;
use std::sync::{Arc, Mutex};

use iced::{
    advanced::text as text_core,
    keyboard::{key::Named, Key},
    widget::{column, container, horizontal_rule, row, text, text_editor, text_input},
    Color, Element, Font, Length, Padding, Size, Task,
};
use numbat::{
    markup::{FormatType, Markup, OutputType},
    module_importer::BuiltinModuleImporter,
    resolver::CodeSource,
    Context, InterpreterResult, InterpreterSettings,
};

// ── colour helpers ───────────────────────────────────────────────────────────
const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
}

// ── colour palette ───────────────────────────────────────────────────────────
const BG: Color            = rgb(0x1a, 0x1a, 0x1a);
const PROMPT_COLOR: Color  = rgb(0x50, 0xfa, 0x7b); // green
const VALUE_COLOR: Color   = rgb(0xf1, 0xfa, 0x8c); // yellow
const UNIT_COLOR: Color    = rgb(0x8b, 0xe9, 0xfd); // cyan
const TYPE_COLOR: Color    = rgb(0xbd, 0x93, 0xf9); // purple
const KEYWORD_COLOR: Color = rgb(0xff, 0x79, 0xc6); // pink
const STRING_COLOR: Color  = rgb(0xf1, 0xfa, 0x8c); // yellow-ish
const DIMMED_COLOR: Color  = rgb(0x66, 0x66, 0x66);
const ERROR_COLOR: Color   = rgb(0xff, 0x55, 0x55);
const NORMAL_COLOR: Color  = rgb(0xf8, 0xf8, 0xf2); // off-white

const FONT_SIZE: f32 = 22.0;
const INPUT_ID: &str = "guimbat_input";

// ── per-token colour spans ───────────────────────────────────────────────────
/// A colored byte range within the full `history_log` string.
#[derive(Clone)]
struct ColorSpan {
    start: usize,
    end: usize,
    color: Color,
}

// ── highlighter ──────────────────────────────────────────────────────────────
/// Settings passed into the highlighter on every content rebuild.
/// `PartialEq` compares only the `generation` counter so iced knows when to
/// reconstruct the highlighter without doing a full span comparison.
#[derive(Clone)]
struct HighlightSettings {
    spans: Vec<ColorSpan>,
    line_starts: Vec<usize>, // byte offset of each line's first character
    generation: u64,
}

impl PartialEq for HighlightSettings {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
    }
}

impl Default for HighlightSettings {
    fn default() -> Self {
        HighlightSettings { spans: Vec::new(), line_starts: vec![0], generation: 0 }
    }
}

struct GuimbatHighlighter {
    settings: HighlightSettings,
    current_line: usize,
}

impl text_core::Highlighter for GuimbatHighlighter {
    type Settings = HighlightSettings;
    type Highlight = Color;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Color)>;

    fn new(settings: &Self::Settings) -> Self {
        GuimbatHighlighter { settings: settings.clone(), current_line: 0 }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.settings = new_settings.clone();
        self.current_line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.current_line = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let line_start =
            self.settings.line_starts.get(self.current_line).copied().unwrap_or(0);
        let line_end = line_start + line.len();

        let ranges: Vec<(Range<usize>, Color)> = self
            .settings
            .spans
            .iter()
            .filter(|s| s.start < line_end && s.end > line_start)
            .map(|s| {
                let local_start = s.start.saturating_sub(line_start);
                let local_end = s.end.min(line_end).saturating_sub(line_start);
                (local_start..local_end, s.color)
            })
            .collect();

        self.current_line += 1;
        ranges.into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

// ── app state ────────────────────────────────────────────────────────────────
struct Guimbat {
    ctx: Context,
    input: String,
    /// Accumulated history as plain text (newline-separated).
    history_log: String,
    /// Colour annotations over `history_log`.
    color_spans: Vec<ColorSpan>,
    history_content: text_editor::Content,
    highlight_settings: HighlightSettings,
    recall: Vec<String>, // input history for ↑/↓
    recall_cursor: Option<usize>,
}

// ── messages ─────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Submit,
    RecallPrev,
    RecallNext,
    HistoryAction(text_editor::Action),
    FocusInput,
    TypeChar(String),
}

// ── initialization ────────────────────────────────────────────────────────────
fn init() -> (Guimbat, Task<Message>) {
    let mut ctx = Context::new(BuiltinModuleImporter {});
    let mut silent = InterpreterSettings { print_fn: Box::new(|_: &Markup| {}) };
    let _ = ctx.interpret_with_settings(&mut silent, "use prelude", CodeSource::Internal);

    let state = Guimbat {
        ctx,
        input: String::new(),
        history_log: String::new(),
        color_spans: Vec::new(),
        history_content: text_editor::Content::new(),
        highlight_settings: HighlightSettings::default(),
        recall: Vec::new(),
        recall_cursor: None,
    };

    (state, text_input::focus(text_input::Id::new(INPUT_ID)))
}

// ── history helpers ───────────────────────────────────────────────────────────
fn push_plain_line(state: &mut Guimbat, line: &str, color: Color) {
    if !state.history_log.is_empty() {
        state.history_log.push('\n');
    }
    let start = state.history_log.len();
    state.history_log.push_str(line);
    state.color_spans.push(ColorSpan { start, end: state.history_log.len(), color });
}

fn push_markup_line(state: &mut Guimbat, markup: &Markup) {
    if !state.history_log.is_empty() {
        state.history_log.push('\n');
    }
    for part in &markup.0 {
        if part.2.contains('\n') && part.2.trim().is_empty() {
            continue;
        }
        let start = state.history_log.len();
        state.history_log.push_str(&*part.2 as &str);
        let end = state.history_log.len();
        state.color_spans.push(ColorSpan { start, end, color: markup_color(&part.0, &part.1) });
    }
}

/// Rebuild `history_content` and `highlight_settings` from the current log.
/// Also moves the editor cursor to the end so it auto-scrolls to new output.
fn sync_history(state: &mut Guimbat) {
    state.history_content = text_editor::Content::with_text(&state.history_log);
    state.history_content
        .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));

    let mut line_starts = vec![0usize];
    for (i, c) in state.history_log.char_indices() {
        if c == '\n' {
            line_starts.push(i + 1);
        }
    }

    state.highlight_settings = HighlightSettings {
        spans: state.color_spans.clone(),
        line_starts,
        generation: state.highlight_settings.generation + 1,
    };
}

fn markup_color(output_type: &OutputType, format_type: &FormatType) -> Color {
    match output_type {
        OutputType::Optional => DIMMED_COLOR,
        OutputType::Normal => match format_type {
            FormatType::Value => VALUE_COLOR,
            FormatType::Unit => UNIT_COLOR,
            FormatType::Identifier => NORMAL_COLOR,
            FormatType::TypeIdentifier => TYPE_COLOR,
            FormatType::Keyword => KEYWORD_COLOR,
            FormatType::String => STRING_COLOR,
            FormatType::Emphasized => NORMAL_COLOR,
            FormatType::Dimmed => DIMMED_COLOR,
            FormatType::Operator => NORMAL_COLOR,
            FormatType::Decorator => TYPE_COLOR,
            FormatType::Text => NORMAL_COLOR,
            FormatType::Whitespace => NORMAL_COLOR,
        },
    }
}

// ── update ────────────────────────────────────────────────────────────────────
fn update(state: &mut Guimbat, message: Message) -> Task<Message> {
    match message {
        Message::InputChanged(s) => {
            state.input = s;
            state.recall_cursor = None;
            Task::none()
        }

        Message::Submit => {
            let raw = state.input.trim().to_owned();
            if raw.is_empty() {
                return Task::none();
            }
            match raw.as_str() {
                "clear" => {
                    state.history_log.clear();
                    state.color_spans.clear();
                    state.history_content = text_editor::Content::new();
                    state.highlight_settings = HighlightSettings {
                        spans: vec![],
                        line_starts: vec![0],
                        generation: state.highlight_settings.generation + 1,
                    };
                    state.input.clear();
                    state.recall_cursor = None;
                    return Task::none();
                }
                "quit" | "exit" => return iced::exit(),
                _ => {}
            }
            evaluate(state, raw);
            text_input::focus(text_input::Id::new(INPUT_ID))
        }

        Message::RecallPrev => {
            recall_prev(state);
            Task::none()
        }

        Message::RecallNext => {
            recall_next(state);
            Task::none()
        }

        // Read-only history: allow selection, block any text edits.
        // Mouse clicks and drags go through this handler.
        Message::HistoryAction(action) => {
            if !action.is_edit() {
                state.history_content.perform(action);
            }
            Task::none()
        }

        // Sent by the history key_binding after Cmd+C so typing can resume.
        Message::FocusInput => text_input::focus(text_input::Id::new(INPUT_ID)),

        // A printable key was pressed while history had focus: forward it.
        Message::TypeChar(s) => {
            state.input.push_str(&s);
            state.recall_cursor = None;
            text_input::focus(text_input::Id::new(INPUT_ID))
        }
    }
}

fn evaluate(state: &mut Guimbat, raw: String) {
    state.recall.push(raw.clone());
    state.recall_cursor = None;
    push_plain_line(state, &format!("  >> {raw}"), PROMPT_COLOR);

    let printed: Arc<Mutex<Vec<Markup>>> = Arc::new(Mutex::new(Vec::new()));
    let printed_clone = Arc::clone(&printed);
    let mut settings = InterpreterSettings {
        print_fn: Box::new(move |m: &Markup| {
            printed_clone.lock().unwrap().push(m.clone());
        }),
    };

    let eval_result =
        state.ctx.interpret_with_settings(&mut settings, &raw, CodeSource::Text);

    drop(settings);
    let printed_lines = Arc::try_unwrap(printed).unwrap().into_inner().unwrap();

    match eval_result {
        Ok((_stmts, result)) => {
            for m in printed_lines {
                push_markup_line(state, &m);
            }
            match result {
                InterpreterResult::Value(_) => {
                    let markup = result.to_markup(
                        None,
                        state.ctx.dimension_registry(),
                        false,
                        true,
                        &numbat::FormatOptions::default(),
                    );
                    push_markup_line(state, &markup);
                }
                InterpreterResult::Continue => {}
            }
        }
        Err(e) => {
            for m in printed_lines {
                push_markup_line(state, &m);
            }
            push_plain_line(state, &format!("  error: {e}"), ERROR_COLOR);
        }
    }

    state.input.clear();
    sync_history(state);
}

fn recall_prev(state: &mut Guimbat) {
    if state.recall.is_empty() {
        return;
    }
    let idx = match state.recall_cursor {
        None => state.recall.len() - 1,
        Some(0) => 0,
        Some(i) => i - 1,
    };
    state.recall_cursor = Some(idx);
    state.input = state.recall[idx].clone();
}

fn recall_next(state: &mut Guimbat) {
    match state.recall_cursor {
        None => {}
        Some(i) if i + 1 >= state.recall.len() => {
            state.recall_cursor = None;
            state.input.clear();
        }
        Some(i) => {
            state.recall_cursor = Some(i + 1);
            state.input = state.recall[i + 1].clone();
        }
    }
}

// ── view ──────────────────────────────────────────────────────────────────────
fn view(state: &Guimbat) -> Element<'_, Message> {
    let history_pane = text_editor(&state.history_content)
        .on_action(Message::HistoryAction)
        .key_binding(|key_press| {
            use text_editor::{Binding, Status};

            // Only intercept when this widget actually has keyboard focus.
            if key_press.status != Status::Focused {
                return None;
            }

            // Cmd+C: copy selection, then return focus to input.
            if let Some(Binding::Copy) = Binding::<Message>::from_key_press(key_press.clone()) {
                return Some(Binding::Sequence(vec![
                    Binding::Copy,
                    Binding::Custom(Message::FocusInput),
                ]));
            }

            // Printable character: forward it to the input field.
            if let Some(text) = &key_press.text {
                let s = text.to_string();
                if !s.is_empty() && !s.chars().all(char::is_control) {
                    return Some(Binding::Custom(Message::TypeChar(s)));
                }
            }

            // Modifier-only key presses (Cmd, Shift, Alt, Ctrl) must not
            // steal focus — the user may be about to press a key combo.
            use iced::keyboard::key::Named;
            if matches!(
                key_press.key,
                iced::keyboard::Key::Named(
                    Named::Super
                        | Named::Shift
                        | Named::Control
                        | Named::Alt
                        | Named::AltGraph
                        | Named::Meta
                )
            ) {
                return None;
            }

            // Any other non-modifier key (arrows, Escape, …): return focus.
            Some(Binding::Custom(Message::FocusInput))
        })
        .font(Font::MONOSPACE)
        .size(FONT_SIZE)
        .height(Length::Fill)
        .padding(Padding::from([4, 8]))
        .highlight_with::<GuimbatHighlighter>(
            state.highlight_settings.clone(),
            |color: &Color, _theme| text_core::highlighter::Format {
                color: Some(*color),
                font: Some(Font::MONOSPACE),
            },
        )
        .style(|_theme, _status| text_editor::Style {
            background: BG.into(),
            border: iced::Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            icon: DIMMED_COLOR,
            placeholder: DIMMED_COLOR,
            value: NORMAL_COLOR,
            selection: rgb(0x44, 0x44, 0x66),
        });

    let input_field = text_input("type an expression…", &state.input)
        .id(text_input::Id::new(INPUT_ID))
        .font(Font::MONOSPACE)
        .size(FONT_SIZE)
        .on_input(Message::InputChanged)
        .on_submit(Message::Submit)
        .style(|_theme, _status| text_input::Style {
            background: iced::Background::Color(BG),
            border: iced::Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            icon: DIMMED_COLOR,
            placeholder: DIMMED_COLOR,
            value: NORMAL_COLOR,
            selection: rgb(0x44, 0x44, 0x66),
        });

    let input_row = row![
        text(">> ")
            .font(Font::MONOSPACE)
            .size(FONT_SIZE)
            .color(PROMPT_COLOR),
        input_field,
    ]
    .align_y(iced::Alignment::Center)
    .padding(Padding::from([6, 8]));

    container(column![history_pane, horizontal_rule(1), input_row])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG)),
            ..Default::default()
        })
        .into()
}

// ── subscription ─────────────────────────────────────────────────────────────
fn subscription(_state: &Guimbat) -> iced::Subscription<Message> {
    iced::keyboard::on_key_press(|key, _mods| match key {
        Key::Named(Named::ArrowUp) => Some(Message::RecallPrev),
        Key::Named(Named::ArrowDown) => Some(Message::RecallNext),
        _ => None,
    })
}

// ── entry point ───────────────────────────────────────────────────────────────
fn main() -> iced::Result {
    iced::application("Guimbat — Numbat", update, view)
        .subscription(subscription)
        .window(iced::window::Settings {
            size: Size::new(700.0, 450.0),
            min_size: Some(Size::new(400.0, 200.0)),
            ..Default::default()
        })
        .run_with(init)
}
