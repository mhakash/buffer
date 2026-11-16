use chrono::{NaiveDate, Utc};
use gpui_kit::{Context, SharedString};

use super::model::{
    CATEGORY_COLORS, Category, DATA_VERSION, Id, Link, LogEntry, Priority, Subtask, Task, TaskData,
    TaskDetails, category_key,
};
use crate::storage;

const FILE: &str = "tasks.json";
/// Copy of a v1 `tasks.json` taken before migrating it to per-task folders.
const V1_BACKUP: &str = "tasks.v1-backup.json";

/// Single source of truth for tasks; every mutation is saved to disk immediately.
/// List fields go to `tasks.json`; each task's details go to its own `tasks/<id>/` folder,
/// so a keystroke in notes rewrites one small file, not every task.
pub struct TaskStore {
    data: TaskData,
    save_error: Option<SharedString>,
}

impl TaskStore {
    pub fn load(_cx: &mut Context<Self>) -> Self {
        Self::from_disk()
    }

    fn from_disk() -> Self {
        let mut data: TaskData = storage::load(FILE);
        let mut store = Self {
            save_error: None,
            data: TaskData {
                version: data.version,
                next_id: data.next_id,
                categories: std::mem::take(&mut data.categories),
                tasks: Vec::new(),
            },
        };
        let inline = data.version < 2;
        for mut task in data.tasks {
            task.migrate_category();
            if !inline {
                let dir = Task::dir(task.id);
                let details: TaskDetails = storage::load(&format!("{dir}/task.json"));
                task.set_details(details, storage::load_text(&format!("{dir}/notes.md")));
            }
            store.data.tasks.push(task);
        }
        if store.data.version != DATA_VERSION {
            store.migrate();
        }
        store
    }

    /// v1 → v2: writes every task's folder, then the slimmed index. The original file is
    /// copied aside first, and the index isn't rewritten unless every folder was written.
    fn migrate(&mut self) {
        if storage::path(FILE).exists()
            && let Err(err) = storage::copy(FILE, V1_BACKUP)
        {
            self.save_error =
                Some(format!("Couldn't back up tasks before upgrading: {err:#}").into());
            return;
        }
        for task in &self.data.tasks {
            if let Err(err) = save_task_files(task) {
                self.save_error = Some(format!("Couldn't upgrade task storage: {err:#}").into());
                return;
            }
        }
        self.data.version = DATA_VERSION;
        self.save_error = storage::save(FILE, &self.data)
            .err()
            .map(|err| format!("Couldn't save tasks: {err:#}").into());
    }

    /// Folder holding the task's `task.json` and `notes.md`.
    pub fn task_dir(id: Id) -> std::path::PathBuf {
        storage::path(&Task::dir(id))
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
        categories: Vec<Id>,
        links: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Id {
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
            categories,
            category: None,
            created_at: Utc::now(),
            completed_at: None,
            due: None,
            updated_at: None,
            notes: String::new(),
            subtasks: Vec::new(),
            links,
            log: Vec::new(),
        });
        self.commit_task(id, cx);
        id
    }

    pub fn update_task(
        &mut self,
        id: Id,
        title: String,
        priority: Priority,
        categories: Vec<Id>,
        cx: &mut Context<Self>,
    ) {
        let Some(existing) = self.task(id).map(|t| t.priority) else {
            return;
        };
        let sort_order = (existing != priority).then(|| self.next_sort_order(priority));
        if let Some(task) = self.task_mut(id) {
            task.title = title;
            task.priority = priority;
            if let Some(order) = sort_order {
                task.sort_order = order;
            }
            task.categories = categories;
            task.updated_at = Some(Utc::now());
            self.commit(cx);
        }
    }

    pub fn toggle_category(&mut self, id: Id, category: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            match task.categories.iter().position(|c| *c == category) {
                Some(ix) => {
                    task.categories.remove(ix);
                }
                None => task.categories.push(category),
            }
            task.updated_at = Some(Utc::now());
            self.commit(cx);
        }
    }

    pub fn set_due(&mut self, id: Id, due: Option<NaiveDate>, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id)
            && task.due != due
        {
            task.due = due;
            task.updated_at = Some(Utc::now());
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
            task.updated_at = Some(Utc::now());
            self.commit(cx);
        }
    }

    pub fn set_notes(&mut self, id: Id, notes: &str, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id)
            && task.notes != notes
        {
            task.notes = notes.to_string();
            task.updated_at = Some(Utc::now());
            self.commit_notes(id, cx);
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
            self.commit_task(id, cx);
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
            self.commit_task(id, cx);
        }
    }

    pub fn delete_subtask(&mut self, id: Id, sub_id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.subtasks.retain(|s| s.id != sub_id);
            self.commit_task(id, cx);
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
            self.commit_task(id, cx);
        }
    }

    pub fn delete_link(&mut self, id: Id, link_id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.links.retain(|l| l.id != link_id);
            self.commit_task(id, cx);
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
            self.commit_task(id, cx);
        }
    }

    pub fn delete_log(&mut self, id: Id, entry_id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task_mut(id) {
            task.log.retain(|e| e.id != entry_id);
            self.commit_task(id, cx);
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
        // Undo restores from the returned copy, which still has every detail.
        if let Err(err) = storage::remove_dir(&Task::dir(id)) {
            self.save_error = Some(format!("Couldn't remove task files: {err:#}").into());
        }
        Some(task)
    }

    pub fn restore_task(&mut self, task: Task, cx: &mut Context<Self>) {
        if self.task(task.id).is_none() {
            let id = task.id;
            self.data.tasks.push(task);
            self.commit_task(id, cx);
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
            task.categories.retain(|c| *c != id);
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

    /// Saves the index (`tasks.json`): list fields, categories, ids.
    fn commit(&mut self, cx: &mut Context<Self>) {
        self.save_error = storage::save(FILE, &self.data)
            .err()
            .map(|err| format!("Couldn't save tasks: {err:#}").into());
        cx.notify();
    }

    /// Saves one task's folder, and the index too since `updated_at` changed.
    fn commit_task(&mut self, id: Id, cx: &mut Context<Self>) {
        self.touch(id);
        if let Some(task) = self.task(id) {
            self.save_error = save_task_files(task)
                .err()
                .map(|err| format!("Couldn't save task: {err:#}").into());
        }
        if self.save_error.is_none() {
            self.commit(cx);
        } else {
            cx.notify();
        }
    }

    /// Saves only `notes.md`: runs on every keystroke, so it leaves the index alone.
    fn commit_notes(&mut self, id: Id, cx: &mut Context<Self>) {
        if let Some(task) = self.task(id) {
            self.save_error =
                storage::save_text(&format!("{}/notes.md", Task::dir(id)), &task.notes)
                    .err()
                    .map(|err| format!("Couldn't save notes: {err:#}").into());
        }
        cx.notify();
    }

    fn touch(&mut self, id: Id) {
        if let Some(task) = self.task_mut(id) {
            task.updated_at = Some(Utc::now());
        }
    }
}

/// Writes `tasks/<id>/task.json` and `notes.md` (removed when empty).
fn save_task_files(task: &Task) -> anyhow::Result<()> {
    let dir = Task::dir(task.id);
    storage::save(&format!("{dir}/task.json"), &task.details())?;
    storage::save_text(&format!("{dir}/notes.md"), &task.notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The only test touching the data dir, so setting the env var can't race another test.
    #[test]
    fn migrates_v1_inline_details_to_task_folders() {
        let dir = std::env::temp_dir().join(format!("buffer-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: no other test reads or writes the environment.
        unsafe { std::env::set_var("BUFFER_DATA_DIR", &dir) };
        let v1 = r##"{"version":1,"next_id":9,"categories":[{"id":1,"name":"Work","color":1}],
            "tasks":[{"id":5,"title":"Ship","priority":"high","category":1,
            "created_at":"2026-01-01T00:00:00Z","notes":"# Plan\n- [ ] a",
            "links":[{"id":6,"url":"https://x.dev"}],"log":[{"id":7,"at":"2026-01-02T00:00:00Z","text":"did"}]}]}"##;
        std::fs::write(dir.join(FILE), v1).unwrap();

        let store = TaskStore::from_disk();
        assert!(store.save_error.is_none(), "{:?}", store.save_error);
        let index = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(index.contains("\"version\": 2") && !index.contains("Plan"));
        assert!(dir.join(V1_BACKUP).exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("tasks/5/notes.md")).unwrap(),
            "# Plan\n- [ ] a"
        );

        // Reload reads details back from the folder.
        let store = TaskStore::from_disk();
        let task = store.task(5).unwrap();
        assert_eq!(task.categories, vec![1]);
        assert_eq!(task.notes, "# Plan\n- [ ] a");
        assert_eq!(task.links.len(), 1);
        assert_eq!(task.log[0].text, "did");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
