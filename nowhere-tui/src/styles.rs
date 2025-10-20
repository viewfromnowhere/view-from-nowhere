use ratatui::style::{Color, Modifier, Style};

pub fn user_header() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn user_text() -> Style {
    Style::default().fg(Color::Cyan)
}

pub fn llm_header() -> Style {
    Style::default()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD)
}

pub fn llm_text() -> Style {
    Style::default().fg(Color::LightGreen)
}

pub fn label() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn value() -> Style {
    Style::default().fg(Color::White)
}

pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn system() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn twitter_header() -> Style {
    Style::default()
        .fg(Color::Blue)
        .add_modifier(Modifier::BOLD)
}

pub fn error() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}
