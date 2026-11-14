use chrono::Utc;
use gpui_kit::{Context, SharedString};

use super::model::{
    CATEGORY_COLORS, Category, Id, Link, LogEntry, Priority, Subtask, Task, TaskData, category_key,
};
use crate::storage;

const FILE: &str = "tasks.json";

/// Single source of truth for tasks; every mutation is saved to disk immediately.
pub struct TaskStore {
    data: TaskData,
    save_error: Option<SharedString>,
}

impl TaskStore {
    pub fn load(_cx: &mut Context<Self>) -> Self {
        Self {
            data: storage::load(FILE),
            save_error: None,
        }
    }

    pub fn tasks(&self) -> &[Task] {
        &self.data.tasks
    }

    pub fn categories(&self) -> &[Category] {
        &self.data.categories
    }

    pub fn task(&self, id: Id) -> Option<&Task> {
        self.data.tasks.iter().find(|t| t.id == id)
    }

    pub fn open_count(&self) -> usize {
        self.data.tasks.iter().filter(|t| !t.is_done()).count()
    }

    pub fn save_error(&self) -> Option<&SharedString> {
        self.save_error.as_ref()
    }

    pub fn add_task(
        &mut self,
        title: String,
        priority: Priority,
        category: Option<Id>,
        links: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let id = self.next_id();
        let sort_order = self.next_sort_order(priority);
        let links = links
            .into_iter()
            .map(|url| Link {
                id: self.next_id(),
                url,
            })
            .collect();
        self.data.tasks.push(Task {
            id,
            title,
            priority,
            sort_order,
            category,
            created_at: Utc::now(),
            completed_at: None,
            notes: String::new(),
            subtasks: Vec::new(),
            links,
            log: Vec::new(),
        });
        self.commit(cx);
    }

    pub fn update_task(
        &mut self,
        id: Id,
        title: String,
        priority: Priority,
        category: Option<Id>,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.task_mut(id) {
            task.title = title;
            task.priority = priority;
            task.category = category;
            self.commit(cx);
        }
    }

    pub fn set_title(&mut self, id: Id, title: &str, cx: &mut Context<Self>) {
        let title = title.trim();
        if let Some(task) = self.task_mut(id)
            && !title.is_empty()
            && task.title != title
        {
            task.title = title.to_string();
            self.commit(cx);
        }
    }

    pub fn set_notes(&mut self, id: Id, notes: &str, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id)
            && task.notes != notes
        {
            task.notes = notes.to_string();
            self.commit(cx);
        }
    }

    pub fn add_subtask(&mut self, id: Id, title: &str, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let sub_id = self.next_id();
        if let Some(task) = self.task_mut(id) {
            task.subtasks.push(Subtask {
                id: sub_id,
                title: title.to_string(),
                completed_at: None,
            });
            self.commit(cx);
        }
    }

    pub fn toggle_subtask(&mut self, id: Id, sub_id: Id, cx: &mut Context<Self>) {
        if let Some(sub) = self
            .task_mut(id)
            .and_then(|t| t.subtasks.iter_mut().find(|s| s.id == sub_id))
        {
            sub.completed_at = match sub.completed_at {
                Some(_) => None,
                None => Some(Utc::now()),
            };
            self.commit(cx);
        }
    }

    pub fn delete_subtask(&mut self, id: Id, sub_id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.subtasks.retain(|s| s.id != sub_id);
            self.commit(cx);
        }
    }

    pub fn add_link(&mut self, id: Id, url: &str, cx: &mut Context<Self>) {
        let url = url.trim();
        if url.is_empty() {
            return;
        }
        let link_id = self.next_id();
        if let Some(task) = self.task_mut(id)
            && !task.links.iter().any(|l| l.url == url)
        {
            task.links.push(Link {
                id: link_id,
                url: url.to_string(),
            });
            self.commit(cx);
        }
    }

    pub fn delete_link(&mut self, id: Id, link_id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.links.retain(|l| l.id != link_id);
            self.commit(cx);
        }
    }

    pub fn add_log(&mut self, id: Id, text: &str, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let entry_id = self.next_id();
        if let Some(task) = self.task_mut(id) {
            task.log.push(LogEntry {
                id: entry_id,
                at: Utc::now(),
                text: text.to_string(),
            });
            self.commit(cx);
        }
    }

    pub fn delete_log(&mut self, id: Id, entry_id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.log.retain(|e| e.id != entry_id);
            self.commit(cx);
        }
    }

    pub fn toggle_done(&mut self, id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.completed_at = match task.completed_at {
                Some(_) => None,
                None => Some(Utc::now()),
            };
            self.commit(cx);
        }
    }

    pub fn set_priority(&mut self, id: Id, priority: Priority, cx: &mut Context<Self>) {
        let Some(existing) = self.task(id).map(|t| t.priority) else {
            return;
        };
        if existing == priority {
            return;
        }
        let sort_order = self.next_sort_order(priority);
        if let Some(task) = self.task_mut(id) {
            task.priority = priority;
            task.sort_order = sort_order;
            self.commit(cx);
        }
    }

    /// Move an open task before or after another open task in the same priority band.
    pub fn reorder_task(
        &mut self,
        dragged_id: Id,
        over_id: Id,
        before: bool,
        cx: &mut Context<Self>,
    ) {
        if dragged_id == over_id {
            return;
        }
        let Some(dragged) = self.task(dragged_id).cloned() else {
            return;
        };
        let Some(over) = self.task(over_id).cloned() else {
            return;
        };
        if dragged.is_done() || over.is_done() || dragged.priority != over.priority {
            return;
        }

        let mut band: Vec<Task> = self
            .data
            .tasks
            .iter()
            .filter(|t| !t.is_done() && t.priority == dragged.priority)
            .cloned()
            .collect();
        band.sort_by_key(|t| (t.sort_order, t.created_at));

        let Some(from_ix) = band.iter().position(|t| t.id == dragged_id) else {
            return;
        };
        let Some(over_ix) = band.iter().position(|t| t.id == over_id) else {
            return;
        };

        let old_order: Vec<Id> = band.iter().map(|t| t.id).collect();
        let mut target_ix = if before { over_ix } else { over_ix + 1 };
        let task = band.remove(from_ix);
        if from_ix < target_ix {
            target_ix = target_ix.saturating_sub(1);
        }
        target_ix = target_ix.min(band.len());
        band.insert(target_ix, task);

        let new_order: Vec<Id> = band.iter().map(|t| t.id).collect();
        if old_order == new_order {
            return;
        }

        for (ix, task) in band.iter().enumerate() {
            if let Some(t) = self.task_mut(task.id) {
                t.sort_order = ix as u64;
            }
        }
        self.commit(cx);
    }

    pub fn delete_task(&mut self, id: Id, cx: &mut Context<Self>) -> Option<Task> {
        let ix = self.data.tasks.iter().position(|t| t.id == id)?;
        let task = self.data.tasks.remove(ix);
        self.commit(cx);
        Some(task)
    }

    pub fn restore_task(&mut self, task: Task, cx: &mut Context<Self>) {
        if self.task(task.id).is_none() {
            self.data.tasks.push(task);
            self.commit(cx);
        }
    }

    /// Returns the existing category matching `name` loosely, or creates it.
    pub fn ensure_category(&mut self, name: &str, cx: &mut Context<Self>) -> Option<Id> {
        let name = name.trim();
        let key = category_key(name);
        if key.is_empty() {
            return None;
        }
        if let Some(existing) = self
            .data
            .categories
            .iter()
            .find(|c| category_key(&c.name) == key)
        {
            return Some(existing.id);
        }
        let id = self.next_id();
        let color = CATEGORY_COLORS[self.data.categories.len() % CATEGORY_COLORS.len()];
        let mut chars = name.chars();
        let name = chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect())
            .unwrap_or_default();
        self.data.categories.push(Category { id, name, color });
        self.commit(cx);
        Some(id)
    }

    /// Deleting a category keeps its tasks; they just become uncategorised.
    pub fn delete_category(&mut self, id: Id, cx: &mut Context<Self>) {
        self.data.categories.retain(|c| c.id != id);
        for task in &mut self.data.tasks {
            if task.category == Some(id) {
                task.category = None;
            }
        }
        self.commit(cx);
    }

    pub fn cycle_category_color(&mut self, id: Id, cx: &mut Context<Self>) {
        if let Some(category) = self.data.categories.iter_mut().find(|c| c.id == id) {
            let ix = CATEGORY_COLORS
                .iter()
                .position(|&c| c == category.color)
                .map_or(0, |ix| (ix + 1) % CATEGORY_COLORS.len());
            category.color = CATEGORY_COLORS[ix];
            self.commit(cx);
        }
    }

    fn task_mut(&mut self, id: Id) -> Option<&mut Task> {
        self.data.tasks.iter_mut().find(|t| t.id == id)
    }

    fn next_id(&mut self) -> Id {
        let id = self.data.next_id;
        self.data.next_id += 1;
        id
    }

    fn next_sort_order(&self, priority: Priority) -> u64 {
        self.data
            .tasks
            .iter()
            .filter(|t| !t.is_done() && t.priority == priority)
            .map(|t| t.sort_order)
            .max()
            .map_or(0, |max| max + 1)
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        self.save_error = storage::save(FILE, &self.data)
            .err()
            .map(|err| format!("Couldn't save tasks: {err:#}").into());
        cx.notify();
    }
}
