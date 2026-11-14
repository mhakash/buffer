use std::collections::BTreeMap;

use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

pub type Id = u64;

/// Declaration order is sort order: most urgent first.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Urgent,
    High,
    #[default]
    Medium,
    Low,
}

impl Priority {
    pub const ALL: [Priority; 4] = [
        Priority::Urgent,
        Priority::High,
        Priority::Medium,
        Priority::Low,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Priority::Urgent => "Urgent",
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
        }
    }

    pub fn color(self) -> u32 {
        match self {
            Priority::Urgent => 0xef4444,
            Priority::High => 0xf59e0b,
            Priority::Medium => 0x3b82f6,
            Priority::Low => 0x94a3b8,
        }
    }

    /// Next priority in a click-to-cycle order (wraps from Low back to Urgent).
    pub fn cycle(self) -> Self {
        match self {
            Priority::Urgent => Priority::High,
            Priority::High => Priority::Medium,
            Priority::Medium => Priority::Low,
            Priority::Low => Priority::Urgent,
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "urgent" | "u" | "1" | "p1" | "!" => Some(Priority::Urgent),
            "high" | "h" | "2" | "p2" => Some(Priority::High),
            "medium" | "med" | "m" | "3" | "p3" => Some(Priority::Medium),
            "low" | "l" | "4" | "p4" => Some(Priority::Low),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    pub id: Id,
    pub name: String,
    pub color: u32,
}

pub const CATEGORY_COLORS: [u32; 8] = [
    0x8b5cf6, 0x10b981, 0xf97316, 0x06b6d4, 0xec4899, 0x84cc16, 0xeab308, 0x6366f1,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub title: String,
    pub priority: Priority,
    /// Order within the same priority band on the To do list (lower = higher).
    #[serde(default)]
    pub sort_order: u64,
    #[serde(default)]
    pub category: Option<Id>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// Free-form working notes.
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
    #[serde(default)]
    pub links: Vec<Link>,
    /// Timestamped progress entries, oldest first.
    #[serde(default)]
    pub log: Vec<LogEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subtask {
    pub id: Id,
    pub title: String,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    pub id: Id,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: Id,
    pub at: DateTime<Utc>,
    pub text: String,
}

impl Task {
    /// (done, total) subtasks.
    pub fn subtask_progress(&self) -> (usize, usize) {
        let done = self
            .subtasks
            .iter()
            .filter(|s| s.completed_at.is_some())
            .count();
        (done, self.subtasks.len())
    }

    pub fn is_done(&self) -> bool {
        self.completed_at.is_some()
    }
}

/// Completed-task count per local calendar day.
pub fn completions_by_day(tasks: &[Task]) -> BTreeMap<NaiveDate, usize> {
    let mut days = BTreeMap::new();
    for ts in tasks.iter().filter_map(|t| t.completed_at) {
        *days
            .entry(ts.with_timezone(&Local).date_naive())
            .or_insert(0) += 1;
    }
    days
}

/// Counts for the `n` days ending `today`, oldest first.
pub fn last_days(by_day: &BTreeMap<NaiveDate, usize>, today: NaiveDate, n: usize) -> Vec<usize> {
    (0..n)
        .rev()
        .map(|back| {
            let day = today - chrono::Days::new(back as u64);
            by_day.get(&day).copied().unwrap_or(0)
        })
        .collect()
}

/// Consecutive days with at least one completion, ending today — or yesterday, so a streak
/// isn't shown as broken before you've had a chance to finish something today.
pub fn streak(by_day: &BTreeMap<NaiveDate, usize>, today: NaiveDate) -> usize {
    let has = |d: NaiveDate| by_day.get(&d).is_some_and(|n| *n > 0);
    let mut day = if has(today) {
        today
    } else {
        today - chrono::Days::new(1)
    };
    let mut count = 0;
    while has(day) {
        count += 1;
        day = day - chrono::Days::new(1);
    }
    count
}

/// On-disk shape of `tasks.json`. Add fields with `#[serde(default)]` to stay backward compatible.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskData {
    pub version: u32,
    pub next_id: Id,
    pub categories: Vec<Category>,
    pub tasks: Vec<Task>,
}

impl Default for TaskData {
    fn default() -> Self {
        let categories: Vec<Category> = ["Work", "Meeting", "Review", "Follow-up"]
            .into_iter()
            .zip(CATEGORY_COLORS)
            .enumerate()
            .map(|(ix, (name, color))| Category {
                id: ix as Id + 1,
                name: name.into(),
                color,
            })
            .collect();
        Self {
            version: 1,
            next_id: categories.len() as Id + 1,
            categories,
            tasks: Vec::new(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct QuickAdd {
    pub title: String,
    /// `http(s)://…` words, removed from the title.
    pub links: Vec<String>,
    pub priority: Option<Priority>,
    pub category: Option<String>,
}

/// Parses `Fix login bug #work !high https://…` into title, category, priority and links.
/// With only a link, the title falls back to the link's label (e.g. `PROJ-123`).
/// Unrecognised `!tokens` stay in the title. Prefix `#` or `!` with `\` for literals (`\#tag`, `\!wow`).
pub fn parse_quick_add(input: &str) -> QuickAdd {
    let mut parsed = QuickAdd::default();
    let mut words = Vec::new();
    for word in input.split_whitespace() {
        if word.starts_with('\\') {
            words.push(unescape_quick_add_word(word));
            continue;
        }
        if is_url(word) {
            parsed.links.push(word.to_string());
        } else if let Some(name) = word.strip_prefix('#').filter(|name| !name.is_empty()) {
            parsed.category = Some(name.to_string());
        } else if let Some(priority) = word.strip_prefix('!').and_then(Priority::parse) {
            parsed.priority = Some(priority);
        } else {
            words.push(word.to_string());
        }
    }
    parsed.title = words.join(" ");
    if parsed.title.is_empty()
        && let Some(url) = parsed.links.first()
    {
        parsed.title = super::links::classify(url).label;
    }
    parsed
}

pub fn is_url(word: &str) -> bool {
    word.starts_with("https://") || word.starts_with("http://")
}

fn unescape_quick_add_word(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('#') => out.push('#'),
            Some('!') => out.push('!'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Comparison key so `#follow-up`, `#followup` and "Follow up" all hit the same category.
pub fn category_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tokens_anywhere() {
        let parsed = parse_quick_add("!urgent Fix login #work bug");
        assert_eq!(parsed.title, "Fix login bug");
        assert_eq!(parsed.priority, Some(Priority::Urgent));
        assert_eq!(parsed.category.as_deref(), Some("work"));
    }

    #[test]
    fn captures_links_and_falls_back_to_link_label() {
        let parsed =
            parse_quick_add("Review https://gitlab.com/g/app/-/merge_requests/412 #review");
        assert_eq!(parsed.title, "Review");
        assert_eq!(parsed.links.len(), 1);
        let parsed = parse_quick_add("https://acme.atlassian.net/browse/PAY-88");
        assert_eq!(parsed.title, "PAY-88");
    }

    #[test]
    fn old_tasks_without_details_load() {
        let json = r#"{"id":1,"title":"x","priority":"high","created_at":"2026-01-01T00:00:00Z"}"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert!(task.notes.is_empty() && task.subtasks.is_empty() && task.links.is_empty());
    }

    #[test]
    fn keeps_unknown_bang_and_bare_hash() {
        let parsed = parse_quick_add("Say hi! # !wow");
        assert_eq!(parsed.title, "Say hi! # !wow");
        assert_eq!(parsed.priority, None);
        assert_eq!(parsed.category, None);
    }

    #[test]
    fn backslash_escapes_hash_and_bang() {
        let parsed = parse_quick_add(r"Post in \#random \!urgent #work");
        assert_eq!(parsed.title, "Post in #random !urgent");
        assert_eq!(parsed.category.as_deref(), Some("work"));
        assert_eq!(parsed.priority, None);
    }

    #[test]
    fn category_names_match_loosely() {
        assert_eq!(category_key("Follow-up"), category_key("follow_up"));
        assert_eq!(category_key("Follow up"), category_key("FOLLOWUP"));
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        let d = |n: u64| today - chrono::Days::new(n);
        let by_day: BTreeMap<_, _> = [(d(1), 2), (d(2), 1), (d(4), 3)].into_iter().collect();
        assert_eq!(streak(&by_day, today), 2);
        let mut with_today = by_day.clone();
        with_today.insert(today, 1);
        assert_eq!(streak(&with_today, today), 3);
        assert_eq!(streak(&BTreeMap::new(), today), 0);
        assert_eq!(last_days(&by_day, today, 5), vec![3, 0, 1, 2, 0]);
    }

    #[test]
    fn data_round_trips() {
        let data = TaskData::default();
        let json = serde_json::to_string(&data).unwrap();
        let back: TaskData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.categories.len(), data.categories.len());
        assert_eq!(back.next_id, data.next_id);
    }
}
