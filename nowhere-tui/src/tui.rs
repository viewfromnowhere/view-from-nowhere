use crate::{
    command::{Command, parse_command},
    styles,
    transcript::TranscriptLine,
    view::{self, ViewSnap},
};
use anyhow::Result;
use async_trait::async_trait;
use crossterm::{
    event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use nowhere_actors::{
    ArtifactRow, BuiltSearchQuery, ChatCmd, ChatResponse, ClaimContext, LlmMsg, SearchCmd,
    StoreMsg,
    actor::{Actor, Addr, Context},
    llm::{ChatLlmActor, LlmActor},
    store::StoreActor,
    system::ShutdownHandle,
    twitter::TwitterSearchActor,
};
use ratatui::{Terminal, backend::CrosstermBackend, style::Style};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tokio::{sync::oneshot, task::JoinHandle};
use uuid::Uuid;

const BRAILLE_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub enum TuiMsg {
    InputEvent(CtEvent),
    Tick,
    Submit(String),
    SearchQueryBuilt(BuiltSearchQuery),
    LlmDone(String),
    ChatDone(ChatResponse),
    TwitterDone(Vec<String>),
    ArtifactsCheckDone(std::result::Result<bool, String>),
    ArtifactsUpdated(Uuid),
    OpError(String),
    ScrollUp,
    ScrollDown,
    Shutdown,
}

pub struct TuiActor {
    claim: Option<ClaimContext>,

    // deps
    llm: Addr<LlmActor>,
    chat_llm: Addr<ChatLlmActor>,
    // FIXME: allow the UI to select from multiple Twitter workers instead of assuming a single dedicated actor.
    twitter: Addr<TwitterSearchActor>,
    store: Addr<StoreActor>,

    // terminal
    term: Terminal<CrosstermBackend<Stdout>>,
    tick_rate: Duration,
    last_tick: Instant,

    // ui state
    input: String,
    input_cursor: usize,
    lines: Vec<TranscriptLine>, // transcript buffer
    scroll: usize,              // from bottom
    dirty: bool,

    // busy/spinner
    busy: u32,
    spin_idx: usize,

    // artifact watch task
    artifact_watch: Option<JoinHandle<()>>,
    artifact_watch_armed: bool,

    // shutdown coordination
    shutdown: ShutdownHandle,
}

impl TuiActor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        llm: Addr<LlmActor>,
        chat_llm: Addr<ChatLlmActor>,
        twitter: Addr<TwitterSearchActor>,
        store: Addr<StoreActor>,
        shutdown: ShutdownHandle,
    ) -> Result<Self> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut term = Terminal::new(backend)?;
        term.clear()?;

        Ok(Self {
            claim: None,
            llm,
            chat_llm,
            twitter,
            store,
            term,
            tick_rate: Duration::from_millis(80),
            last_tick: Instant::now(),
            input: String::new(),
            input_cursor: 0,
            lines: vec![TranscriptLine::new(
                "Write '/claim' before entering an empirical claim to investigate.".into(),
                styles::system(),
            )],
            scroll: 0,
            dirty: true,
            busy: 0,
            spin_idx: 0,
            artifact_watch: None,
            artifact_watch_armed: false,
            shutdown,
        })
    }

    fn cursor_left(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        self.input_cursor -= 1;
        while self.input_cursor > 0 && !self.input.is_char_boundary(self.input_cursor) {
            self.input_cursor -= 1;
        }
    }

    fn cursor_right(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        self.input_cursor += 1;
        while self.input_cursor < self.input.len()
            && !self.input.is_char_boundary(self.input_cursor)
        {
            self.input_cursor += 1;
        }
    }

    fn cursor_home(&mut self) {
        self.input_cursor = 0;
    }

    fn cursor_end(&mut self) {
        self.input_cursor = self.input.len();
    }

    fn insert_char(&mut self, ch: char) {
        self.input.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();
    }

    fn backspace(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let mut prev = self.input_cursor.saturating_sub(1);
        while prev > 0 && !self.input.is_char_boundary(prev) {
            prev -= 1;
        }
        self.input.drain(prev..self.input_cursor);
        self.input_cursor = prev;
    }

    fn delete(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        let start = self.input_cursor;
        let mut end = start + 1;
        while end < self.input.len() && !self.input.is_char_boundary(end) {
            end += 1;
        }
        self.input.drain(start..end);
    }

    pub fn set_claim(&mut self, ctx: ClaimContext) {
        self.claim = Some(ctx);
    }

    pub fn clear_claim(&mut self) {
        self.cancel_artifact_watch();
        self.claim = None;
    }

    fn cancel_artifact_watch(&mut self) {
        if let Some(handle) = self.artifact_watch.take() {
            handle.abort();
        }
        self.artifact_watch_armed = false;
    }

    fn subscribe_artifact_updates(&mut self, claim: &ClaimContext, me: Addr<TuiActor>) {
        self.cancel_artifact_watch();
        let store = self.store.clone();
        let claim_id = claim.id;
        let handle = tokio::spawn(async move {
            let (tx, rx) = oneshot::channel();
            match store
                .send(StoreMsg::WatchArtifacts {
                    claim: claim_id,
                    reply: tx,
                })
                .await
            {
                Ok(_) => {
                    if rx.await.is_ok() {
                        let _ = me.send(TuiMsg::ArtifactsUpdated(claim_id)).await;
                    }
                }
                Err(_) => {
                    let _ = me
                        .send(TuiMsg::OpError("store watch registration failed".into()))
                        .await;
                }
            }
        });
        self.artifact_watch = Some(handle);
        self.artifact_watch_armed = true;
    }

    fn push<S: Into<String>>(&mut self, s: S) {
        self.push_styled(s, Style::default());
    }

    fn push_styled<S: Into<String>>(&mut self, s: S, style: Style) {
        self.lines.push(TranscriptLine::new(s.into(), style));
        self.dirty = true;
    }

    fn push_blank(&mut self) {
        self.push(String::new());
    }

    fn render_chat(&mut self, resp: ChatResponse) {
        self.push_styled("← [Nowhere]", styles::llm_header());
        for line in resp.text.lines() {
            self.push_styled(format!("  {line}"), styles::llm_text());
        }

        if !resp.used_artifacts.is_empty() {
            self.push_styled("  Artifacts:", styles::label());
            for art in resp.used_artifacts {
                self.push_styled(format!("    • {art}"), styles::value());
            }
        } else {
            self.push_styled("  Artifacts: (none)", styles::dim());
        }

        if !resp.used_entities.is_empty() {
            self.push_styled("  Entities:", styles::label());
            for ent in resp.used_entities {
                self.push_styled(format!("    • {ent}"), styles::value());
            }
        } else {
            self.push_styled("  Entities: (none)", styles::dim());
        }

        if !resp.caveats.is_empty() {
            self.push_styled("  Caveats:", styles::label());
            for c in resp.caveats {
                self.push_styled(format!("    • {c}"), styles::value());
            }
        }

        self.push_blank();
    }

    fn spinner(&self) -> &'static str {
        if self.busy > 0 {
            BRAILLE_FRAMES[self.spin_idx % BRAILLE_FRAMES.len()]
        } else {
            " "
        }
    }

    fn set_busy(&mut self, on: bool) {
        if on {
            self.busy = self.busy.saturating_add(1)
        } else {
            self.busy = self.busy.saturating_sub(1)
        }
        self.dirty = true;
    }

    fn step_spinner(&mut self) {
        if self.busy > 0 {
            self.spin_idx = (self.spin_idx + 1) % BRAILLE_FRAMES.len();
            self.dirty = true;
        }
    }

    fn draw(&mut self) -> Result<()> {
        let snap = ViewSnap::new(
            self.input.clone(),
            self.input_cursor,
            self.lines.clone(),
            self.scroll,
            self.busy,
            self.spinner(),
        );

        view::draw(&mut self.term, &snap)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<TuiMsg> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
            | (KeyCode::Char('q'), KeyModifiers::CONTROL) => return Some(TuiMsg::Shutdown),
            (KeyCode::PageUp, _) => {
                self.scroll = self.scroll.saturating_add(5);
                self.dirty = true;
            }
            (KeyCode::PageDown, _) => {
                self.scroll = self.scroll.saturating_sub(5);
                self.dirty = true;
            }
            (KeyCode::Up, _) => {
                self.scroll = self.scroll.saturating_add(1);
                self.dirty = true;
            }
            (KeyCode::Down, _) => {
                self.scroll = self.scroll.saturating_sub(1);
                self.dirty = true;
            }
            (KeyCode::Enter, _) => {
                let line = std::mem::take(&mut self.input);
                self.input_cursor = 0;
                self.dirty = true;
                return Some(TuiMsg::Submit(line));
            }
            (KeyCode::Left, _) => {
                self.cursor_left();
                self.dirty = true;
            }
            (KeyCode::Right, _) => {
                self.cursor_right();
                self.dirty = true;
            }
            (KeyCode::Home, _) => {
                self.cursor_home();
                self.dirty = true;
            }
            (KeyCode::End, _) => {
                self.cursor_end();
                self.dirty = true;
            }
            (KeyCode::Backspace, _) => {
                self.backspace();
                self.dirty = true;
            }
            (KeyCode::Delete, _) => {
                self.delete();
                self.dirty = true;
            }
            (KeyCode::Esc, _) => {
                self.input.clear();
                self.input_cursor = 0;
                self.dirty = true;
            }
            (KeyCode::Char(ch), _) => {
                self.insert_char(ch);
                self.dirty = true;
            }
            _ => {}
        }
        None
    }

    fn route_submit(&mut self, line: String, me: Addr<TuiActor>) {
        let s = line.trim().to_string();
        if s.is_empty() {
            return;
        }

        if s.starts_with('/') {
            let cmd = parse_command(&s);
            self.handle_command(cmd, me);
            return;
        }

        if let Some(claim) = self.claim.clone() {
            self.push_styled("→ [You]", styles::user_header());
            for line in s.lines() {
                self.push_styled(format!("  {line}"), styles::user_text());
            }
            self.push_blank();
            self.set_busy(true);
            let (tx, rx) = oneshot::channel::<ChatResponse>();
            let _ = self.chat_llm.try_send(ChatCmd {
                user_text: s.clone(),
                k: 25,
                claim,
                reply: tx,
            });
            let me2 = me.clone();
            tokio::spawn(async move {
                match rx.await {
                    Ok(resp) => {
                        let _ = me2.send(TuiMsg::ChatDone(resp)).await;
                    }
                    Err(e) => {
                        let _ = me2.send(TuiMsg::OpError(format!("chat: {e}"))).await;
                    }
                }
            });
            return;
        }

        self.push_styled(
            "× No claim selected. Use `/claim <text>` first, or `/quit`.",
            styles::error(),
        );
    }

    fn check_for_artifacts(&mut self, claim: &ClaimContext, me: Addr<TuiActor>, announce: bool) {
        if announce {
            self.push_styled("collecting artifacts", styles::system());
        }
        self.set_busy(true);

        let store = self.store.clone();
        let me2 = me;
        let claim_id = claim.id;
        tokio::spawn(async move {
            let (tx, rx) = oneshot::channel::<Result<Vec<ArtifactRow>>>();
            let msg = StoreMsg::SearchArtifacts {
                claim: claim_id,
                query: String::new(),
                limit: 1,
                reply: tx,
            };

            let result: std::result::Result<bool, String> = match store.send(msg).await {
                Ok(_) => match rx.await {
                    Ok(Ok(rows)) => Ok(!rows.is_empty()),
                    Ok(Err(e)) => Err(format!("store query: {e}")),
                    Err(e) => Err(format!("store channel: {e}")),
                },
                Err(_) => Err("store mailbox dropped".into()),
            };

            let _ = me2.send(TuiMsg::ArtifactsCheckDone(result)).await;
        });
    }

    fn active_claim_text(&self) -> Option<String> {
        self.claim.as_ref().map(|c| c.text.clone())
    }

    fn handle_command(&mut self, cmd: Command, me: Addr<TuiActor>) {
        match cmd {
            Command::Quit => {
                let _ = me.try_send(TuiMsg::Shutdown);
            }
            Command::Help => {
                self.push_styled("Commands:", styles::label());
                self.push_styled("  /claim <text>   set the active claim", styles::value());
                self.push_styled("  /claim          show the active claim", styles::value());
                self.push_styled("  /claim -        clear the active claim", styles::value());
                self.push_styled("  /quit           exit", styles::value());
                self.push_blank();
            }
            Command::Claim(None) => {
                if let Some(text) = self.active_claim_text() {
                    self.push_styled("Active claim:", styles::label());
                    self.push_styled(format!("  {text}"), styles::value());
                } else {
                    self.push_styled("No active claim. Use `/claim <text>`.", styles::dim());
                }
                self.push_blank();
            }
            Command::Claim(Some(text)) => {
                if text.is_empty() {
                    self.clear_claim();
                    self.push_styled("✓ Cleared active claim.", styles::system());
                    self.push_blank();
                    return;
                }

                let claim = ClaimContext {
                    id: Uuid::new_v4(),
                    text: text.clone(),
                };
                self.set_claim(claim.clone());

                let _ = self.store.try_send(StoreMsg::InsertClaim(claim.clone()));
                self.push_styled("→ [Claim]", styles::user_header());
                self.push_styled(format!("  {text}"), styles::user_text());
                self.push_blank();

                self.check_for_artifacts(&claim, me.clone(), true);
                self.subscribe_artifact_updates(&claim, me.clone());

                self.set_busy(true);
                let (tx, rx) = oneshot::channel::<BuiltSearchQuery>();
                let _ = self.llm.try_send(LlmMsg::BuildSearchQuery {
                    claim: claim.clone(),
                    reply: tx,
                });

                let me2 = me.clone();
                tokio::spawn(async move {
                    match rx.await {
                        Ok(response) => {
                            let _ = me2.send(TuiMsg::SearchQueryBuilt(response)).await;
                        }
                        Err(e) => {
                            let _ = me2.send(TuiMsg::OpError(format!("llm: {e}"))).await;
                        }
                    }
                });
            }
            Command::Unknown(s) => {
                self.push_styled(format!("× Unknown command: {s}"), styles::error());
                self.push_styled("Try `/help`.", styles::dim());
                self.push_blank();
            }
        }
    }
}

#[async_trait]
impl Actor for TuiActor {
    type Msg = TuiMsg;

    async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
        match msg {
            TuiMsg::InputEvent(ev) => {
                if let CtEvent::Key(k) = ev
                    && let Some(next) = self.handle_key(k)
                {
                    let _ = ctx.addr().try_send(next);
                }
            }
            TuiMsg::Submit(line) => self.route_submit(line, ctx.addr()),
            TuiMsg::SearchQueryBuilt(built_search_query) => {
                let _ = self
                    .twitter
                    .send(SearchCmd {
                        query: built_search_query.query,
                        date_from: built_search_query.date_from,
                        date_to: built_search_query.date_to,
                        claim: built_search_query.claim,
                    })
                    .await;
            }
            TuiMsg::LlmDone(text) => {
                self.push_styled("← [Nowhere]", styles::llm_header());
                for line in text.lines() {
                    self.push_styled(format!("  {line}"), styles::llm_text());
                }
                self.push_blank();
                self.set_busy(false);
            }
            TuiMsg::ChatDone(resp) => {
                self.render_chat(resp);
                self.set_busy(false);
            }
            TuiMsg::TwitterDone(v) => {
                self.push_styled(
                    format!("← [Twitter] {} result(s)", v.len()),
                    styles::twitter_header(),
                );
                if v.is_empty() {
                    self.push_styled("  (no tweets yet)", styles::dim());
                } else {
                    self.push_styled("  Top results:", styles::label());
                    for t in v.clone().into_iter().take(5) {
                        self.push_styled(format!("    • {t}"), styles::value());
                    }
                    if v.len() > 5 {
                        self.push_styled(format!("    • … {} more", v.len() - 5), styles::dim());
                    }
                }
                self.push_blank();
                self.set_busy(false);
            }
            TuiMsg::ArtifactsCheckDone(result) => {
                match result {
                    Ok(true) => {
                        self.push_styled(
                            "Artifacts are present. What would you like to know?",
                            styles::system(),
                        );
                        self.cancel_artifact_watch();
                    }
                    Ok(false) => {
                        self.push_styled("No artifacts found yet.", styles::dim());
                        if let Some(claim) = self.claim.clone() {
                            if !self.artifact_watch_armed {
                                let addr = ctx.addr();
                                self.subscribe_artifact_updates(&claim, addr);
                            }
                        }
                    }
                    Err(e) => {
                        self.push_styled(
                            format!("× Error checking artifacts: {e}"),
                            styles::error(),
                        );
                        if let Some(claim) = self.claim.clone() {
                            if !self.artifact_watch_armed {
                                let addr = ctx.addr();
                                self.subscribe_artifact_updates(&claim, addr);
                            }
                        }
                    }
                }
                self.push_blank();
                self.set_busy(false);
            }
            TuiMsg::ArtifactsUpdated(claim_id) => {
                if let Some(claim) = self.claim.clone() {
                    if claim.id == claim_id {
                        self.artifact_watch = None;
                        self.artifact_watch_armed = false;
                        let addr = ctx.addr();
                        self.check_for_artifacts(&claim, addr.clone(), false);
                    }
                }
            }
            TuiMsg::OpError(e) => {
                self.push_styled(format!("× Error: {e}"), styles::error());
                self.push_blank();
                self.set_busy(false);
            }
            TuiMsg::Tick => {
                self.step_spinner();
                if self.dirty || self.last_tick.elapsed() >= self.tick_rate {
                    self.draw()?;
                    self.last_tick = Instant::now();
                    self.dirty = false;
                }
            }
            TuiMsg::ScrollUp => {
                self.scroll = self.scroll.saturating_add(1);
                self.dirty = true;
            }
            TuiMsg::ScrollDown => {
                self.scroll = self.scroll.saturating_sub(1);
                self.dirty = true;
            }
            TuiMsg::Shutdown => {
                disable_raw_mode().ok();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                self.shutdown.signal();
                ctx.stop();
            }
        }

        Ok(())
    }
}
