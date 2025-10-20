use crate::transcript::TranscriptLine;
use anyhow::Result;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::io::Stdout;
use textwrap::wrap;

pub struct ViewSnap {
    pub input: String,
    pub input_cursor: usize,
    pub lines: Vec<TranscriptLine>,
    pub scroll: usize,
    pub busy: u32,
    pub spinner: &'static str,
}

impl ViewSnap {
    pub fn new(
        input: String,
        input_cursor: usize,
        lines: Vec<TranscriptLine>,
        scroll: usize,
        busy: u32,
        spinner: &'static str,
    ) -> Self {
        Self {
            input,
            input_cursor,
            lines,
            scroll,
            busy,
            spinner,
        }
    }
}

pub fn draw(term: &mut Terminal<CrosstermBackend<Stdout>>, snap: &ViewSnap) -> Result<()> {
    term.draw(|frame| {
        let area = frame.area();

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(Line::from(vec![Span::styled(
            " View From Nowhere ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]))
        .wrap(Wrap { trim: true });
        frame.render_widget(header, layout[0]);

        // Transcript window
        let visible_h = layout[1].height.saturating_sub(2) as usize;
        let content_width = layout[1].width.saturating_sub(2) as usize;
        let wrapped = wrap_transcript(&snap.lines, content_width);
        let total = wrapped.len();
        let start = total.saturating_sub(visible_h + snap.scroll);
        let end = total.saturating_sub(snap.scroll);

        let items: Vec<ListItem> = wrapped[start..end]
            .iter()
            .map(|(text, style)| {
                let line = Line::from(Span::styled(text.clone(), style.clone()));
                ListItem::new(line)
            })
            .collect();

        let body =
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Transcript "));
        frame.render_widget(body, layout[1]);

        // Input box
        let input_box = Paragraph::new(snap.input.clone())
            .block(Block::default().borders(Borders::ALL).title(" Input "));
        frame.render_widget(Clear, layout[2]);
        frame.render_widget(input_box, layout[2]);

        // Caret placement — uses snapshot, not `self`
        let caret_x = layout[2].x + 1 + visual_caret_col(&snap.input, snap.input_cursor);
        let caret_y = layout[2].y + 1;
        frame.set_cursor_position(Position {
            x: caret_x,
            y: caret_y,
        });

        // Status bar
        let status_line = Line::from(vec![
            Span::raw(" "),
            Span::styled(snap.spinner, Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            if snap.busy > 0 {
                Span::styled("Working…", Style::default().fg(Color::Yellow))
            } else {
                Span::styled("Idle", Style::default().fg(Color::Green))
            },
            Span::raw(format!(" • ops: {}", snap.busy)),
        ]);
        let status = Paragraph::new(status_line)
            .block(Block::default().borders(Borders::ALL).title(" Status "));
        frame.render_widget(status, layout[3]);
    })?;

    Ok(())
}

fn visual_caret_col(input: &str, cursor: usize) -> u16 {
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(&input[..cursor]) as u16
}

fn wrap_transcript(lines: &[TranscriptLine], width: usize) -> Vec<(String, Style)> {
    let effective_width = width.max(1);
    let mut out = Vec::new();

    for entry in lines {
        let style = entry.style;
        if entry.text.is_empty() {
            out.push((String::new(), style));
            continue;
        }

        for raw_line in entry.text.split('\n') {
            if raw_line.is_empty() {
                out.push((String::new(), style));
                continue;
            }

            let segments = wrap(raw_line, effective_width);
            if segments.is_empty() {
                out.push((String::new(), style));
            } else {
                out.extend(segments.into_iter().map(|seg| (seg.into_owned(), style)));
            }
        }
    }

    out
}
