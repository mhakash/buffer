use chrono::{DateTime, Utc};
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
    #[serde(default)]
    pub category: Option<Id>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn is_done(&self) -> bool {
        self.completed_at.is_some()
    }
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
    pub priority: Option<Priority>,
    pub category: Option<String>,
}

/// Parses `Fix login bug #work !high` into title, category and priority.
/// Unrecognised `!tokens` stay in the title.
pub fn parse_quick_add(input: &str) -> QuickAdd {
    let mut parsed = QuickAdd::default();
    let mut words = Vec::new();
    for word in input.split_whitespace() {
        if let Some(name) = word.strip_prefix('#').filter(|name| !name.is_empty()) {
            parsed.category = Some(name.to_string());
        } else if let Some(priority) = word.strip_prefix('!').and_then(Priority::parse) {
            parsed.priority = Some(priority);
        } else {
            words.push(word);
        }
    }
    parsed.title = words.join(" ");
    parsed
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
    fn keeps_unknown_bang_and_bare_hash() {
        let parsed = parse_quick_add("Say hi! # !wow");
        assert_eq!(parsed.title, "Say hi! # !wow");
        assert_eq!(parsed.priority, None);
        assert_eq!(parsed.category, None);
    }

    #[test]
    fn category_names_match_loosely() {
        assert_eq!(category_key("Follow-up"), category_key("follow_up"));
        assert_eq!(category_key("Follow up"), category_key("FOLLOWUP"));
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
