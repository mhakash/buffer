//! Files in the content: `/` menu items (upload, link, new text file) and files dropped onto
//! the editor become Markdown links, one block each. Uploaded and new files are copied into the
//! host's files folder ([`BlockEditor::set_files`]) and linked relatively; linked files keep
//! their absolute path. Without a files folder, everything is linked in place.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::{
    Context, DragMoveEvent, ExternalPaths, PathPromptOptions, Window, assets::IconName,
};

use super::{BlockEditor, model::Kind};
use crate::{components::file_editor, files};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileAction {
    Upload,
    Link,
    Text,
}

/// `/` menu rows for files: label, keywords, icon.
pub(super) fn file_items() -> [(FileAction, &'static str, &'static str, IconName); 3] {
    [
        (
            FileAction::Upload,
            "Upload file",
            "file upload attach document pdf image",
            IconName::Upload,
        ),
        (
            FileAction::Link,
            "Link file",
            "file link reference path document",
            IconName::Link,
        ),
        (
            FileAction::Text,
            "Text file",
            "file txt json code snippet paste dump new",
            IconName::FilePlus,
        ),
    ]
}

/// Where new link blocks go.
#[derive(Clone, Copy)]
enum Place {
    /// Fill this (empty) block, the rest after it.
    Into(u64),
    /// Insert before index.
    At(usize),
}

impl BlockEditor {
    /// Files added to the content are copied into `sub` (relative to the editor's folder);
    /// `None` links them where they are.
    pub fn set_files(&mut self, sub: Option<String>) {
        self.files = sub;
    }

    fn files_dir(&self) -> Option<PathBuf> {
        self.files.as_ref().map(|sub| self.dir.join(sub))
    }

    /// A file item picked in the `/` menu on block `ix` (left empty by the menu).
    pub(super) fn file_action(
        &mut self,
        ix: usize,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.doc.blocks.get(ix) else {
            return;
        };
        let place = Place::Into(block.id);
        self.deactivate(cx);
        match action {
            FileAction::Upload | FileAction::Link => {
                let link = action == FileAction::Link;
                let paths = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: true,
                    prompt: Some(if link { "Link" } else { "Upload" }.into()),
                });
                cx.spawn(async move |this, cx| {
                    if let Ok(Ok(Some(paths))) = paths.await {
                        this.update(cx, |this, cx| this.add_files(place, &paths, link, cx))
                            .ok();
                    }
                })
                .detach();
            }
            FileAction::Text => self.new_text_file(place, window, cx),
        }
    }

    /// Makes `untitled.txt`, links it and opens it with its name selected; renaming it in the
    /// dialog rewrites the link.
    fn new_text_file(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        let (dir, sub) = match (self.files_dir(), self.files.clone()) {
            (Some(dir), Some(sub)) => (dir, sub),
            _ => return,
        };
        let name = match files::create_empty(&dir, "untitled.txt") {
            Ok(name) => name,
            Err(err) => {
                eprintln!("couldn't create a text file: {err:#}");
                return;
            }
        };
        self.insert_links(
            place,
            vec![files::markdown_link(&name, &format!("{sub}/{name}"))],
            cx,
        );
        let editor = cx.entity().downgrade();
        let on_rename: file_editor::OnRename = Rc::new(move |old, new, _, cx| {
            let sub = sub.clone();
            editor
                .update(cx, |this, cx| this.relink(&sub, &old, &new, cx))
                .ok();
        });
        file_editor::open(dir.join(name), Some(on_rename), true, window, cx);
    }

    /// Links to `old` (in `sub`) now point at `new`.
    fn relink(&mut self, sub: &str, old: &Path, new: &Path, cx: &mut Context<Self>) {
        let name = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        let (old, new) = (name(old), name(new));
        let (old_target, new_target) = (format!("{sub}/{old}"), format!("{sub}/{new}"));
        let (old_md, new_md) = (
            files::markdown_link(&old, &old_target),
            files::markdown_link(&new, &new_target),
        );
        let mut changed = false;
        for block in &mut self.doc.blocks {
            if block.text.contains(&old_target) {
                let text = block
                    .text
                    .replace(&old_md, &new_md)
                    .replace(&format!("(<{old_target}>)"), &format!("(<{new_target}>)"));
                block.set_text(text);
                changed = true;
            }
        }
        if changed {
            self.changed(cx);
        }
    }

    /// Copies (or, `link`, references) `paths` and inserts a link block for each.
    fn add_files(&mut self, place: Place, paths: &[PathBuf], link: bool, cx: &mut Context<Self>) {
        let target_dir = self.files_dir().filter(|_| !link);
        let links: Vec<String> = paths
            .iter()
            .filter(|p| p.is_file())
            .filter_map(|path| match (&target_dir, &self.files) {
                (Some(dir), Some(sub)) => match files::copy_into(path, dir) {
                    Ok(name) => Some(files::markdown_link(&name, &format!("{sub}/{name}"))),
                    Err(err) => {
                        eprintln!("couldn't add {}: {err:#}", path.display());
                        None
                    }
                },
                _ => {
                    let name = path.file_name()?.to_string_lossy().to_string();
                    Some(files::markdown_link(&name, &path.to_string_lossy()))
                }
            })
            .collect();
        self.insert_links(place, links, cx);
    }

    fn insert_links(&mut self, place: Place, links: Vec<String>, cx: &mut Context<Self>) {
        if links.is_empty() {
            return;
        }
        self.checkpoint();
        let mut links = links.into_iter();
        let at = match place {
            Place::Into(id) => match self.index_of(id) {
                Some(ix) if self.doc.blocks[ix].text.trim().is_empty() => {
                    let first = links.next().unwrap_or_default();
                    self.doc.blocks[ix].kind = Kind::Paragraph;
                    self.doc.blocks[ix].set_text(first);
                    ix + 1
                }
                Some(ix) => ix + 1,
                None => self.doc.blocks.len(),
            },
            Place::At(ix) => ix.min(self.doc.blocks.len()),
        };
        for (i, text) in links.enumerate() {
            let block = self.doc.block(Kind::Paragraph, &text);
            self.doc.blocks.insert(at + i, block);
        }
        self.selection = None;
        self.changed(cx);
    }

    /// Files dragged from Finder over the editor: show where they'll land.
    pub(super) fn on_drag_files(
        &mut self,
        event: &DragMoveEvent<ExternalPaths>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pos = event.event.position;
        let at = event.bounds.contains(&pos).then(|| self.drop_index(pos.y));
        if at != self.drop_at {
            self.drop_at = at;
            cx.notify();
        }
    }

    pub(super) fn on_drop_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self
            .drop_at
            .take()
            .unwrap_or_else(|| self.drop_index(window.mouse_position().y));
        self.deactivate(cx);
        self.add_files(Place::At(at), paths.paths(), false, cx);
    }
}
