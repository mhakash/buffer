//! Pomodoro-style focus timer. One `Entity<FocusTimer>` lives in `Workspace`; the sidebar and the
//! Focus page both observe it. Finished focus sessions are logged to `focus.json`.

use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use gpui_kit::{Context, Task};
use serde::{Deserialize, Serialize};

use crate::storage;

const FILE: &str = "focus.json";
pub const LENGTHS: [u32; 4] = [15, 25, 45, 60];
const SHORT_BREAK_MIN: u32 = 5;
const LONG_BREAK_MIN: u32 = 15;
const SESSIONS_PER_LONG_BREAK: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Focus => "Focus",
            Phase::ShortBreak => "Short break",
            Phase::LongBreak => "Long break",
        }
    }

    pub fn is_break(self) -> bool {
        self != Phase::Focus
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub ended_at: DateTime<Utc>,
    pub minutes: u32,
}

/// On-disk shape of `focus.json`. New fields need `#[serde(default)]`.
#[derive(Debug, Serialize, Deserialize)]
struct FocusData {
    #[serde(default = "default_minutes")]
    focus_minutes: u32,
    #[serde(default)]
    sessions: Vec<Session>,
}

fn default_minutes() -> u32 {
    25
}

impl Default for FocusData {
    fn default() -> Self {
        Self {
            focus_minutes: default_minutes(),
            sessions: Vec::new(),
        }
    }
}

pub struct FocusTimer {
    data: FocusData,
    phase: Phase,
    /// Time left while paused; recomputed from `ends_at` while running.
    remaining: Duration,
    ends_at: Option<Instant>,
    last_shown_secs: u64,
    _ticker: Option<Task<()>>,
}

impl FocusTimer {
    pub fn load(_cx: &mut Context<Self>) -> Self {
        let data: FocusData = storage::load(FILE);
        let remaining = minutes(data.focus_minutes);
        Self {
            data,
            phase: Phase::Focus,
            remaining,
            ends_at: None,
            last_shown_secs: remaining.as_secs(),
            _ticker: None,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_running(&self) -> bool {
        self.ends_at.is_some()
    }

    /// True once started or paused mid-way (anything but a fresh phase).
    pub fn is_active(&self) -> bool {
        self.is_running() || self.remaining() < self.total()
    }

    pub fn focus_minutes(&self) -> u32 {
        self.data.focus_minutes
    }

    pub fn remaining(&self) -> Duration {
        match self.ends_at {
            Some(end) => end.saturating_duration_since(Instant::now()),
            None => self.remaining,
        }
    }

    pub fn total(&self) -> Duration {
        minutes(match self.phase {
            Phase::Focus => self.data.focus_minutes,
            Phase::ShortBreak => SHORT_BREAK_MIN,
            Phase::LongBreak => LONG_BREAK_MIN,
        })
    }

    /// Elapsed fraction of the current phase, 0..=1.
    pub fn progress(&self) -> f32 {
        let total = self.total().as_secs_f32();
        if total <= 0. {
            return 0.;
        }
        1. - self.remaining().as_secs_f32() / total
    }

    /// `mm:ss`, rounding up so a running timer never shows 00:00 early.
    pub fn clock(&self) -> String {
        let secs = self.remaining().as_secs_f32().ceil() as u64;
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }

    pub fn sessions_today(&self) -> usize {
        self.today().count()
    }

    pub fn minutes_today(&self) -> u32 {
        self.today().map(|s| s.minutes).sum()
    }

    pub fn total_sessions(&self) -> usize {
        self.data.sessions.len()
    }

    fn today(&self) -> impl Iterator<Item = &Session> {
        let today = Local::now().date_naive();
        self.data
            .sessions
            .iter()
            .filter(move |s| s.ended_at.with_timezone(&Local).date_naive() == today)
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.is_running() {
            self.remaining = self.remaining();
            self.ends_at = None;
            self._ticker = None;
        } else {
            self.ends_at = Some(Instant::now() + self.remaining);
            self._ticker = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    match this.update(cx, |timer, cx| timer.tick(cx)) {
                        Ok(true) => continue,
                        _ => break,
                    }
                }
            }));
        }
        cx.notify();
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.ends_at = None;
        self._ticker = None;
        self.remaining = self.total();
        cx.notify();
    }

    /// Ends the current phase now (a skipped focus session is not logged).
    pub fn skip(&mut self, cx: &mut Context<Self>) {
        self._ticker = None;
        self.advance(false, cx);
    }

    pub fn set_focus_minutes(&mut self, value: u32, cx: &mut Context<Self>) {
        self.data.focus_minutes = value;
        self.save();
        if self.phase == Phase::Focus && !self.is_active() {
            self.remaining = self.total();
        }
        cx.notify();
    }

    /// Returns whether the ticker should keep running.
    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_running() {
            return false;
        }
        if self.remaining().is_zero() {
            self.advance(true, cx);
            chime();
            return false;
        }
        let shown = self.remaining().as_secs();
        if shown != self.last_shown_secs {
            self.last_shown_secs = shown;
            cx.notify();
        }
        true
    }

    fn advance(&mut self, finished: bool, cx: &mut Context<Self>) {
        self.phase = match self.phase {
            Phase::Focus => {
                if finished {
                    self.data.sessions.push(Session {
                        ended_at: Utc::now(),
                        minutes: self.data.focus_minutes,
                    });
                    self.save();
                }
                let done = self.sessions_today();
                if done > 0 && done % SESSIONS_PER_LONG_BREAK == 0 {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                }
            }
            Phase::ShortBreak | Phase::LongBreak => Phase::Focus,
        };
        self.ends_at = None;
        self.remaining = self.total();
        cx.notify();
    }

    fn save(&self) {
        if let Err(err) = storage::save(FILE, &self.data) {
            eprintln!("failed to save focus log: {err:#}");
        }
    }
}

fn minutes(m: u32) -> Duration {
    Duration::from_secs(m as u64 * 60)
}

fn chime() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .spawn();
    }
}
