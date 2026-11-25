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

/// Where a task stands. Derived from `completed_at` / `follow_up_at`, not stored itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Todo,
    /// Not actionable now: waiting on someone, to be followed up.
    FollowUp,
    Done,
}

impl Status {
    pub const ALL: [Status; 3] = [Status::Todo, Status::FollowUp, Status::Done];

    pub fn label(self) -> &'static str {
        match self {
            Status::Todo => "To do",
            Status::FollowUp => "Follow up",
            Status::Done => "Done",
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

/// One task. The index (`tasks.json`) holds the fields shown in lists; the rest lives in the
/// task's own folder (`tasks/<id>/`, see [`TaskDetails`]) so editing notes rewrites only that task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub title: String,
    pub priority: Priority,
    /// Order within the same priority band on the To do list (lower = higher).
    #[serde(default)]
    pub sort_order: u64,
    #[serde(default)]
    pub categories: Vec<Id>,
    /// Single category of pre-0.7 files; folded into `categories` on load.
    #[serde(default, skip_serializing)]
    pub category: Option<Id>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// Set while the task waits for a follow-up (since when); cleared when done or back to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    /// Last edit to the task or its details.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    // Details: stored in `tasks/<id>/`. `default` (without `skip_deserializing`) still reads them
    // from pre-0.7 files, where they were inline, so they can be migrated.
    /// Markdown notes (`tasks/<id>/notes.md`).
    #[serde(default, skip_serializing)]
    pub notes: String,
    /// Pre-0.27 subtasks, links and work log: read only to move them into the notes
    /// ([`Task::fold_legacy`]); they're checklists, links and log lines in the notes now.
    #[serde(default, skip_serializing)]
    pub subtasks: Vec<Subtask>,
    #[serde(default, skip_serializing)]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing)]
    pub log: Vec<LogEntry>,
}

/// On-disk shape of `tasks/<id>/task.json`; notes are beside it in `notes.md`. The lists are
/// only in pre-0.27 files (moved into the notes on load) and are no longer written.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TaskDetails {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtasks: Vec<Subtask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<LogEntry>,
}

impl Task {
    pub fn dir(id: Id) -> String {
        format!("tasks/{id}")
    }

    pub fn details(&self) -> TaskDetails {
        TaskDetails {
            subtasks: self.subtasks.clone(),
            links: self.links.clone(),
            log: self.log.clone(),
        }
    }

    pub fn set_details(&mut self, details: TaskDetails, notes: String) {
        self.subtasks = details.subtasks;
        self.links = details.links;
        self.log = details.log;
        self.notes = notes;
    }

    /// Moves a pre-0.7 single `category` into `categories`.
    pub fn migrate_category(&mut self) {
        if let Some(id) = self.category.take()
            && !self.categories.contains(&id)
        {
            self.categories.insert(0, id);
        }
    }

    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.is_done() && self.due.is_some_and(|d| d < today)
    }
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
    /// Moves pre-0.27 subtasks, links and work log into the notes as Markdown (a checklist,
    /// a link list, a log newest first). True if there was anything to move.
    pub fn fold_legacy(&mut self) -> bool {
        if self.subtasks.is_empty() && self.links.is_empty() && self.log.is_empty() {
            return false;
        }
        let mut sections = Vec::new();
        if !self.subtasks.is_empty() {
            let items: Vec<String> = self
                .subtasks
                .iter()
                .map(|s| {
                    let mark = if s.completed_at.is_some() { "x" } else { " " };
                    format!("- [{mark}] {}", s.title)
                })
                .collect();
            sections.push(format!("## Subtasks\n\n{}", items.join("\n")));
        }
        if !self.links.is_empty() {
            let items: Vec<String> = self
                .links
                .iter()
                .map(|l| format!("- {}", link_markdown(&l.url)))
                .collect();
            sections.push(format!("## Links\n\n{}", items.join("\n")));
        }
        if !self.log.is_empty() {
            let items: Vec<String> = self
                .log
                .iter()
                .rev()
                .map(|e| format!("- {}{}", log_stamp(e.at), e.text))
                .collect();
            sections.push(format!("## Work log\n\n{}", items.join("\n")));
        }
        let notes = self.notes.trim_end();
        let joined = sections.join("\n\n");
        self.notes = if notes.is_empty() {
            format!("{joined}\n")
        } else {
            format!("{notes}\n\n{joined}\n")
        };
        self.subtasks.clear();
        self.links.clear();
        self.log.clear();
        true
    }

    /// (done, total) checklist items (`- [ ]` / `- [x]`) in the notes.
    pub fn todo_progress(&self) -> (usize, usize) {
        let mut in_fence = false;
        let (mut done, mut total) = (0, 0);
        for line in self.notes.lines() {
            let line = line.trim_start();
            if line.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            let Some(rest) = line.strip_prefix(['-', '*', '+']) else {
                continue;
            };
            if in_fence {
                continue;
            }
            let rest = rest.trim_start();
            if rest.starts_with("[ ]") {
                total += 1;
            } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
                total += 1;
                done += 1;
            }
        }
        (done, total)
    }

    /// Web links in the notes (Markdown link targets and bare URLs), in order, no duplicates.
    pub fn web_links(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for word in self
            .notes
            .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>'))
        {
            let start = word.find("http://").or_else(|| word.find("https://"));
            if let Some(start) = start {
                let url = word[start..].trim_end_matches(['.', ',', ';', ']', '"', '\'']);
                if url.len() > 8 && !out.iter().any(|u| u == url) {
                    out.push(url.to_string());
                }
            }
        }
        out
    }

    pub fn is_done(&self) -> bool {
        self.completed_at.is_some()
    }

    pub fn status(&self) -> Status {
        if self.is_done() {
            Status::Done
        } else if self.follow_up_at.is_some() {
            Status::FollowUp
        } else {
            Status::Todo
        }
    }

    /// Moves the task to `status`, keeping existing timestamps when it's already there.
    pub fn set_status(&mut self, status: Status, now: DateTime<Utc>) {
        match status {
            Status::Todo => {
                self.completed_at = None;
                self.follow_up_at = None;
            }
            Status::FollowUp => {
                self.completed_at = None;
                self.follow_up_at.get_or_insert(now);
            }
            Status::Done => {
                self.completed_at.get_or_insert(now);
                self.follow_up_at = None;
            }
        }
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

/// Index format version: 1 = details inline in `tasks.json`; 2 = details in `tasks/<id>/`.
pub const DATA_VERSION: u32 = 2;

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
            version: DATA_VERSION,
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
    /// Every `#name`, in order, without loose duplicates.
    pub categories: Vec<String>,
}

/// Parses `Fix login bug #work #review !high https://…` into title, categories, priority and links.
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
            if !parsed
                .categories
                .iter()
                .any(|c| category_key(c) == category_key(name))
            {
                parsed.categories.push(name.to_string());
            }
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

/// `[PAY-88](https://…/browse/PAY-88)`: a link labelled the way lists show it.
pub fn link_markdown(url: &str) -> String {
    let label = super::links::classify(url).label.replace(['[', ']'], "");
    format!("[{label}]({url})")
}

/// `**Nov 20, 14:05** — `: start of a work-log line.
pub fn log_stamp(at: DateTime<Utc>) -> String {
    format!(
        "**{}** — ",
        at.with_timezone(&Local).format("%b %-d, %H:%M")
    )
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
    fn status_round_trips_and_old_files_load() {
        let json = r#"{"id":1,"title":"t","priority":"high","created_at":"2026-01-01T00:00:00Z"}"#;
        let mut task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.status(), Status::Todo);
        let now = Utc::now();
        task.set_status(Status::FollowUp, now);
        assert_eq!(task.status(), Status::FollowUp);
        let saved = serde_json::to_string(&task).unwrap();
        let back: Task = serde_json::from_str(&saved).unwrap();
        assert_eq!(back.follow_up_at, Some(now));
        task.set_status(Status::Done, now);
        assert_eq!((task.status(), task.follow_up_at), (Status::Done, None));
        task.set_status(Status::Todo, now);
        assert_eq!((task.status(), task.completed_at), (Status::Todo, None));
        assert!(
            !serde_json::to_string(&task)
                .unwrap()
                .contains("follow_up_at")
        );
    }

    #[test]
    fn parses_tokens_anywhere() {
        let parsed = parse_quick_add("!urgent Fix login #work bug");
        assert_eq!(parsed.title, "Fix login bug");
        assert_eq!(parsed.priority, Some(Priority::Urgent));
        assert_eq!(parsed.categories, vec!["work"]);
    }

    #[test]
    fn collects_several_categories() {
        let parsed = parse_quick_add("Ship #work #Review #review");
        assert_eq!(parsed.title, "Ship");
        assert_eq!(parsed.categories, vec!["work", "Review"]);
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
    fn legacy_details_fold_into_notes() {
        let json = r#"{"id":1,"title":"x","priority":"high","created_at":"2026-01-01T00:00:00Z",
            "notes":"Plan\n",
            "subtasks":[{"id":2,"title":"a"},{"id":3,"title":"b","completed_at":"2026-01-01T00:00:00Z"}],
            "links":[{"id":4,"url":"https://jira.x.com/browse/PAY-88"}],
            "log":[{"id":5,"at":"2026-01-01T10:00:00Z","text":"first"},
                   {"id":6,"at":"2026-01-02T10:00:00Z","text":"second"}]}"#;
        let mut task: Task = serde_json::from_str(json).unwrap();
        assert!(task.fold_legacy());
        assert!(task.subtasks.is_empty() && task.links.is_empty() && task.log.is_empty());
        assert!(
            task.notes
                .starts_with("Plan\n\n## Subtasks\n\n- [ ] a\n- [x] b\n\n## Links")
        );
        assert!(
            task.notes
                .contains("- [PAY-88](https://jira.x.com/browse/PAY-88)")
        );
        let second = task.notes.find("second").unwrap();
        assert!(second < task.notes.find("first").unwrap(), "newest first");
        assert!(!task.fold_legacy());
        assert_eq!(task.todo_progress(), (1, 2));
        assert_eq!(task.web_links(), ["https://jira.x.com/browse/PAY-88"]);
    }

    #[test]
    fn notes_derive_progress_and_links() {
        let mut task: Task = serde_json::from_str(
            r#"{"id":1,"title":"x","priority":"high","created_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        task.notes = "- [ ] a\n  * [X] b\n```\n- [ ] code\n```\nsee https://x.dev/a, \
                      [mr](<https://g.com/-/merge_requests/4>)"
            .into();
        assert_eq!(task.todo_progress(), (1, 2));
        assert_eq!(
            task.web_links(),
            ["https://x.dev/a", "https://g.com/-/merge_requests/4"]
        );
    }

    #[test]
    fn old_tasks_without_details_load() {
        let json = r#"{"id":1,"title":"x","priority":"high","created_at":"2026-01-01T00:00:00Z"}"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert!(task.notes.is_empty() && task.subtasks.is_empty() && task.links.is_empty());
    }

    #[test]
    fn v1_inline_details_load_but_are_not_written_to_the_index() {
        let json = r#"{"id":1,"title":"x","priority":"high","category":4,
            "created_at":"2026-01-01T00:00:00Z","notes":"hi",
            "subtasks":[{"id":2,"title":"s"}]}"#;
        let mut task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.notes, "hi");
        assert_eq!(task.subtasks.len(), 1);
        task.migrate_category();
        assert_eq!(task.categories, vec![4]);
        let out = serde_json::to_string(&task).unwrap();
        assert!(
            !out.contains("notes") && !out.contains("subtasks") && !out.contains("\"category\"")
        );
    }

    #[test]
    fn keeps_unknown_bang_and_bare_hash() {
        let parsed = parse_quick_add("Say hi! # !wow");
        assert_eq!(parsed.title, "Say hi! # !wow");
        assert_eq!(parsed.priority, None);
        assert!(parsed.categories.is_empty());
    }

    #[test]
    fn backslash_escapes_hash_and_bang() {
        let parsed = parse_quick_add(r"Post in \#random \!urgent #work");
        assert_eq!(parsed.title, "Post in #random !urgent");
        assert_eq!(parsed.categories, vec!["work"]);
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
