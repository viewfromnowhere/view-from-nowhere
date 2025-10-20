use ratatui::style::Style;

#[derive(Clone)]
pub struct TranscriptLine {
    pub text: String,
    pub style: Style,
}

impl TranscriptLine {
    pub fn new(text: String, style: Style) -> Self {
        Self { text, style }
    }
}
