#[derive(Debug, Clone)]
pub enum Command {
    Claim(Option<String>), // /claim <text> | /claim | /claim -
    Help,                  // /help
    Quit,                  // /quit or /exit
    Unknown(String),
}

pub fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return Command::Unknown(trimmed.to_string());
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let verb = parts.next().unwrap_or_default();
    let rest = parts.next().map(str::trim).filter(|s| !s.is_empty());

    match verb {
        "/claim" => match rest {
            None => Command::Claim(None),
            Some("-") => Command::Claim(Some(String::new())),
            Some(text) => Command::Claim(Some(text.to_string())),
        },
        "/help" => Command::Help,
        "/quit" | "/exit" => Command::Quit,
        _ => Command::Unknown(trimmed.to_string()),
    }
}
