//! Standalone Markdown notes, Notion-style: a list pane and a document pane that fill the
//! content area. The title is always editable; the body is a Notion-style block editor
//! (`editor`), or its Markdown source. Full view hides the list. Files are links in the
//! content (`/file`, drag and drop); the ones it references are listed at the end.

use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Size, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    h_flex,
    input::{Escape, Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    AnyElement, Context, Entity, FocusHandle, FontWeight, SharedString, Subscription, Window,
    assets::IconName, div, prelude::*, px, rgb,
};

use super::task_detail::when;
use crate::{
    components::{category_tag, confirm::confirm_delete, file_editor, icon_button, property_row},
    editor::{BlockEditor, BlockEditorEvent},
    files,
    notes::{Note, NoteId, NoteStore},
    tasks::{TaskStore, is_url},
};

const LIST_W: f32 = 272.;
/// Reading width of the document; full view allows a little more.
const DOC_W: f32 = 720.;
const DOC_FULL_W: f32 = 860.;

pub struct NotesPage {
    store: Entity<NoteStore>,
    /// Source of the tag list (shared task categories).
    tasks: Entity<TaskStore>,
    selected: Option<NoteId>,
    /// Body shows the Markdown source instead of the block editor.
    source: bool,
    blocks: Entity<BlockEditor>,
    /// List hidden, document takes the whole area.
    full: bool,
    title: Entity<InputState>,
    editor: Entity<TextareaState>,
    search: Entity<InputState>,
    /// Property whose value is being edited in `prop_input`.
    editing_prop: Option<usize>,
    prop_input: Entity<InputState>,
    /// "Add a property" name field is open.
    adding_prop: bool,
    new_prop: Entity<InputState>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl NotesPage {
    pub fn new(tasks: Entity<TaskStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(NoteStore::load);
        let prop_input = cx.new(|cx| InputState::new(window, cx).placeholder("Empty"));
        let new_prop = cx.new(|cx| InputState::new(window, cx).placeholder("Property name"));
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled"));
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(12, 5000)
                .placeholder("Write in Markdown: # headings, - [ ] checklists, `code`, links…")
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let blocks = cx.new(|cx| BlockEditor::new(window, cx));
        let subscriptions = vec![
            // Keep a selection whenever there's a note (first launch, after a delete).
            cx.observe_in(&store, window, |this, _, window, cx| {
                this.ensure_selection(window, cx);
                cx.notify();
            }),
            cx.observe(&search, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &blocks,
                window,
                |this, _, event: &BlockEditorEvent, window, cx| match event {
                    BlockEditorEvent::Changed(markdown) => {
                        if let Some(id) = this.selected {
                            this.store
                                .update(cx, |s, cx| s.set_content(id, markdown, cx));
                        }
                    }
                    BlockEditorEvent::Exit => this.focus_handle.focus(window, cx),
                },
            ),
            cx.observe(&tasks, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &prop_input,
                window,
                |this, input, event: &InputEvent, _, cx| {
                    let (Some(id), Some(ix)) = (this.selected, this.editing_prop) else {
                        return;
                    };
                    match event {
                        InputEvent::Change => {
                            let value = input.read(cx).value();
                            this.store
                                .update(cx, |s, cx| s.set_prop_value(id, ix, &value, cx));
                        }
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            this.editing_prop = None;
                            cx.notify();
                        }
                        _ => {}
                    }
                },
            ),
            cx.subscribe_in(
                &new_prop,
                window,
                |this, input, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => {
                        let name = input.read(cx).value().to_string();
                        let Some(id) = this.selected else { return };
                        this.adding_prop = false;
                        if name.trim().is_empty() {
                            cx.notify();
                            return;
                        }
                        this.store.update(cx, |s, cx| s.add_prop(id, &name, cx));
                        let ix = this
                            .store
                            .read(cx)
                            .note(id)
                            .map_or(0, |n| n.props.len().saturating_sub(1));
                        this.edit_prop(ix, window, cx);
                    }
                    InputEvent::Blur => {
                        this.adding_prop = false;
                        cx.notify();
                    }
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &title,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    let Some(id) = this.selected else { return };
                    match event {
                        InputEvent::Change => {
                            let value = input.read(cx).value();
                            this.store.update(cx, |s, cx| s.set_title(id, &value, cx));
                        }
                        InputEvent::PressEnter { .. } if this.source => {
                            this.editor.update(cx, |e, cx| e.focus(window, cx));
                        }
                        InputEvent::PressEnter { .. } => {
                            this.blocks.update(cx, |b, cx| b.focus_start(window, cx));
                        }
                        _ => {}
                    }
                },
            ),
            cx.subscribe_in(&editor, window, |this, input, event: &InputEvent, _, cx| {
                if let (InputEvent::Change, Some(id), true) = (event, this.selected, this.source) {
                    let value = input.read(cx).value();
                    this.store.update(cx, |s, cx| s.set_content(id, &value, cx));
                }
            }),
        ];
        let mut this = Self {
            store,
            tasks,
            editing_prop: None,
            prop_input,
            adding_prop: false,
            new_prop,
            selected: None,
            source: false,
            blocks,
            full: false,
            title,
            editor,
            search,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.ensure_selection(window, cx);
        this
    }

    /// Called by `Workspace::navigate`: something rendered must hold focus.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.source {
            self.editor.update(cx, |e, cx| e.focus(window, cx));
        } else if !self.blocks.update(cx, |b, cx| b.focus(window, cx)) {
            self.focus_handle.focus(window, cx);
        }
    }

    pub fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_if_empty(cx);
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        let id = self.store.update(cx, |s, cx| s.create(cx));
        self.show(id, window, cx);
        self.title.update(cx, |t, cx| t.focus(window, cx));
    }

    fn select(&mut self, id: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            self.drop_if_empty(cx);
            self.show(id, window, cx);
            self.focus_handle.focus(window, cx);
        }
    }

    fn show(&mut self, id: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.source = false;
        self.editing_prop = None;
        self.adding_prop = false;
        let (title, content) = self
            .store
            .read(cx)
            .note(id)
            .map(|n| {
                let parts = n.split();
                (parts.title.to_string(), parts.content.to_string())
            })
            .unwrap_or_default();
        self.title
            .update(cx, |t, cx| t.set_value(title, window, cx));
        self.blocks.update(cx, |b, cx| {
            b.load(&content, NoteStore::dir(), cx);
            b.set_files(Some(NoteStore::files_subdir(id)));
        });
        cx.notify();
    }

    fn ensure_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        if self.selected.is_some_and(|id| store.note(id).is_some()) {
            return;
        }
        match store.notes().first().map(|n| n.id) {
            Some(id) => self.show(id, window, cx),
            None => {
                self.selected = None;
                self.source = false;
            }
        }
    }

    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(content) = self
            .selected
            .and_then(|id| self.store.read(cx).note(id))
            .map(|n| n.split().content.to_string())
        else {
            return;
        };
        // The store is the source of truth while the block editor shows.
        self.source = true;
        self.editor.update(cx, |e, cx| {
            e.set_value(content, window, cx);
            e.focus(window, cx);
        });
        cx.notify();
    }

    /// Back from the source to the block editor.
    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.source = false;
        let content = self
            .selected
            .and_then(|id| self.store.read(cx).note(id))
            .map(|n| n.split().content.to_string())
            .unwrap_or_default();
        self.blocks
            .update(cx, |b, cx| b.load(&content, NoteStore::dir(), cx));
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn edit_prop(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(value) = self
            .selected
            .and_then(|id| self.store.read(cx).note(id)?.props.get(ix).cloned())
            .map(|p| p.value)
        else {
            return;
        };
        self.editing_prop = Some(ix);
        self.prop_input.update(cx, |i, cx| {
            i.set_value(value, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    fn add_prop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.adding_prop = true;
        self.new_prop.update(cx, |i, cx| {
            i.set_value("", window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    /// Esc: close a property field, leave the editor, then full view.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_prop.is_some() || self.adding_prop {
            self.editing_prop = None;
            self.adding_prop = false;
            self.focus_handle.focus(window, cx);
            cx.notify();
        } else if self.source {
            self.preview(window, cx);
        } else if self.full {
            self.full = false;
            cx.notify();
        }
    }

    /// A note left blank isn't worth keeping: leaving it removes it.
    fn drop_if_empty(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected
            && self.store.read(cx).note(id).is_some_and(|n| {
                n.text.trim().is_empty() && n.tags.is_empty() && n.props.is_empty()
            })
        {
            self.selected = None;
            self.source = false;
            self.store.update(cx, |s, cx| s.delete(id, cx));
        }
    }

    fn render_list(
        &self,
        notes: &[Note],
        count: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let header = h_flex()
            .gap_2()
            .pl_4()
            .pr_2()
            .pt_3()
            .pb_2()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .child("Notes"),
            )
            .when(count > 0, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(count.to_string()),
                )
            })
            .child(div().flex_1())
            .child(
                icon_button(
                    "new-note",
                    IconName::Plus,
                    theme.muted_foreground,
                    "New note (⌘⇧N)",
                )
                .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
            );

        let search = h_flex()
            .mx_3()
            .mb_2()
            .gap_2()
            .px_3()
            .rounded_xl()
            .bg(theme.secondary)
            .text_sm()
            .child(
                Icon::new(IconName::Search)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.search).appearance(false).cleanable(true)),
            );

        let rows = notes.iter().map(|note| {
            let id = note.id;
            let selected = self.selected == Some(id);
            let snippet = note.snippet();
            v_flex()
                .id(("note-row", id))
                .px_3()
                .py_2()
                .gap_0p5()
                .rounded_xl()
                .cursor_pointer()
                .when(selected, |s| s.bg(theme.secondary_active))
                .when(!selected, |s| s.hover(|s| s.bg(theme.secondary)))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(note.title()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(if snippet.is_empty() {
                            when(note.updated_at)
                        } else {
                            format!("{} · {snippet}", when(note.updated_at))
                        }),
                )
                .on_click(cx.listener(move |this, _, window, cx| this.select(id, window, cx)))
        });
        let no_match = notes.is_empty() && count > 0;

        v_flex()
            .flex_none()
            .w(px(LIST_W))
            .h_full()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(header)
            .child(search)
            .child(
                div()
                    .id("notes-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .child(v_flex().gap_0p5().children(rows))
                    .when(no_match, |this| {
                        this.child(
                            div()
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("No matching notes"),
                        )
                    }),
            )
    }

    fn render_toolbar(
        &self,
        note: Option<&Note>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let full = self.full;
        let source = self.source;
        h_flex()
            .gap_1()
            .px_3()
            .pt_3()
            .child(
                icon_button(
                    "notes-full",
                    if full {
                        IconName::PanelLeftOpen
                    } else {
                        IconName::PanelLeftClose
                    },
                    theme.muted_foreground,
                    if full { "Show list" } else { "Full view" },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.full = !full;
                    cx.notify();
                })),
            )
            .child(div().flex_1())
            .when_some(note, |this, note| {
                let id = note.id;
                let title = note.title();
                this.child(
                    Button::new("note-mode")
                        .ghost()
                        .small()
                        .rounded(ButtonRounded::Size(px(999.)))
                        .icon(
                            Icon::new(if source {
                                IconName::Type
                            } else {
                                IconName::SquareCode
                            })
                            .text_color(theme.muted_foreground),
                        )
                        .label(if source { "Rich text" } else { "Markdown" })
                        .tooltip(if source {
                            "Back to the block editor"
                        } else {
                            "Edit the Markdown source"
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.source {
                                this.preview(window, cx);
                            } else {
                                this.edit(window, cx);
                            }
                        })),
                )
                .child(
                    icon_button(
                        "note-delete",
                        IconName::Trash,
                        theme.muted_foreground,
                        "Delete note",
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let store = this.store.clone();
                        confirm_delete(
                            window,
                            cx,
                            "Delete note?",
                            format!("“{title}” will be deleted."),
                            move |_, cx| store.update(cx, |s, cx| s.delete(id, cx)),
                        );
                    })),
                )
            })
    }

    /// Notion-style properties under the title: dates, tags (shared task categories) and
    /// free-form text properties. Click a value to edit; hover a row to remove it.
    fn render_properties(
        &self,
        note: &Note,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = note.id;
        let categories = self.tasks.read(cx).categories().to_vec();
        let tags: Vec<_> = categories
            .iter()
            .filter(|c| note.tags.contains(&c.id))
            .cloned()
            .collect();

        let store = self.store.clone();
        let tasks = self.tasks.clone();
        let picker = Button::new("note-tags")
            .ghost()
            .xsmall()
            .rounded(ButtonRounded::Size(px(999.)))
            .icon(Icon::new(IconName::Plus).text_color(theme.muted_foreground))
            .when(tags.is_empty(), |b| b.label("Add"))
            .tooltip("Tags")
            .dropdown_menu(move |mut menu, _, cx| {
                let current = store
                    .read(cx)
                    .note(id)
                    .map(|n| n.tags.clone())
                    .unwrap_or_default();
                let all = tasks.read(cx).categories().to_vec();
                if all.is_empty() {
                    return menu.label("No categories yet — add some in Settings");
                }
                for category in all {
                    let cid = category.id;
                    let store = store.clone();
                    menu = menu.item(
                        PopupMenuItem::new(category.name)
                            .checked(current.contains(&cid))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| s.toggle_tag(id, cid, cx));
                            }),
                    );
                }
                menu.min_w(px(180.))
            });

        let props =
            note.props.iter().enumerate().map(|(ix, prop)| {
                let group = SharedString::from(format!("note-prop-{ix}"));
                let value = if self.editing_prop == Some(ix) {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .child(Input::new(&self.prop_input).appearance(false).px_0())
                        .into_any_element()
                } else {
                    let url = is_url(prop.value.trim()).then(|| prop.value.trim().to_string());
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .id(("note-prop-value", ix))
                                .flex_1()
                                .min_w_0()
                                .py_1()
                                .cursor_text()
                                .text_sm()
                                .truncate()
                                .when(prop.value.is_empty(), |d| {
                                    d.text_color(theme.muted_foreground).child("Empty")
                                })
                                .when(url.is_some(), |d| d.text_color(theme.primary))
                                .when(!prop.value.is_empty(), |d| d.child(prop.value.clone()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.edit_prop(ix, window, cx)
                                })),
                        )
                        .when_some(url, |this, url| {
                            this.child(
                                icon_button(
                                    ("note-prop-open", ix),
                                    IconName::ExternalLink,
                                    theme.muted_foreground,
                                    "Open link",
                                )
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                            )
                        })
                        .into_any_element()
                };
                let name = prop.name.clone();
                property_row(IconName::FileText, prop.name.clone(), theme)
                    .group(group.clone())
                    .child(value)
                    .child(
                        div()
                            .opacity(0.)
                            .group_hover(group, |s| s.opacity(1.))
                            .child(
                                icon_button(
                                    ("note-prop-del", ix),
                                    IconName::X,
                                    theme.muted_foreground,
                                    "Remove property",
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let store = this.store.clone();
                                    confirm_delete(
                                        window,
                                        cx,
                                        "Remove property?",
                                        format!(
                                            "“{name}” and its value will be removed from this note."
                                        ),
                                        move |_, cx| {
                                            store.update(cx, |s, cx| s.remove_prop(id, ix, cx))
                                        },
                                    );
                                })),
                            ),
                    )
            });

        let add = if self.adding_prop {
            property_row(IconName::Plus, "", theme)
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .child(Input::new(&self.new_prop).appearance(false).px_0()),
                )
                .into_any_element()
        } else {
            h_flex()
                .id("note-add-prop")
                .h(px(32.))
                .gap_2()
                .cursor_pointer()
                .text_sm()
                .text_color(theme.muted_foreground)
                .opacity(0.7)
                .hover(|s| s.opacity(1.))
                .child(
                    Icon::new(IconName::Plus)
                        .small()
                        .text_color(theme.muted_foreground),
                )
                .child("Add a property")
                .on_click(cx.listener(|this, _, window, cx| this.add_prop(window, cx)))
                .into_any_element()
        };

        v_flex()
            .pb_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                property_row(IconName::Clock, "Created", theme)
                    .child(div().text_sm().child(when(note.created_at))),
            )
            .child(
                property_row(IconName::Pencil, "Edited", theme)
                    .child(div().text_sm().child(when(note.updated_at))),
            )
            .child(
                property_row(IconName::Tags, "Tags", theme).child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(
                            tags.iter()
                                .map(|c| category_tag(c.name.clone(), rgb(c.color).into())),
                        )
                        .child(picker),
                ),
            )
            .children(props)
            .child(add)
    }

    fn render_body(&self) -> AnyElement {
        if self.source {
            // XSmall = the kit's smallest editor padding (4px, no vertical); text size and a
            // -4px margin put the source where the rendered text was.
            return div()
                .ml(px(-4.))
                .child(
                    Textarea::new(&self.editor)
                        .appearance(false)
                        .with_size(Size::XSmall)
                        .text_size(px(16.)),
                )
                .into_any_element();
        }
        self.blocks.clone().into_any_element()
    }

    /// Files the note links to (uploaded, linked or made in the app), read from its content:
    /// click opens one (text files in the editor dialog), hover shows it in Finder.
    fn render_files(&self, note: &Note, theme: &Theme) -> Option<AnyElement> {
        let links = files::local_links(note.split().content);
        if links.is_empty() {
            return None;
        }
        let dir = NoteStore::dir();
        let rows = links.into_iter().enumerate().map(|(ix, link)| {
            let path = files::resolve(&link.target, &dir);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| link.target.clone());
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file());
            let missing = meta.is_none();
            let inside = path.starts_with(&dir);
            let detail = match (&meta, inside) {
                (None, _) => "Missing".to_string(),
                (Some(m), true) => files::human_size(m.len()),
                (Some(m), false) => format!("Linked · {}", files::human_size(m.len())),
            };
            let icon = match files::kind(&name) {
                files::Kind::Text => IconName::FileText,
                files::Kind::Image => IconName::FileImage,
                files::Kind::Other => IconName::File,
            };
            let group = SharedString::from(format!("note-file-{ix}"));
            let (open, reveal) = (path.clone(), path.clone());
            h_flex()
                .id(("note-file", ix))
                .group(group.clone())
                .h(px(30.))
                .px_2()
                .ml(px(-8.))
                .gap_2()
                .rounded_xl()
                .cursor_pointer()
                .hover(|s| s.bg(theme.secondary))
                .child(Icon::new(icon).small().text_color(if missing {
                    theme.muted_foreground
                } else {
                    theme.chart_1
                }))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .when(missing, |d| {
                            d.text_color(theme.muted_foreground).line_through()
                        })
                        .child(name),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                )
                .child(div().flex_1())
                .when(!missing, |row| {
                    row.child(
                        div()
                            .opacity(0.)
                            .group_hover(group, |s| s.opacity(1.))
                            .child(
                                icon_button(
                                    ("note-file-reveal", ix),
                                    IconName::FolderOpen,
                                    theme.muted_foreground,
                                    "Show in Finder",
                                )
                                .xsmall()
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    cx.reveal_path(&reveal);
                                }),
                            ),
                    )
                })
                .on_click(move |_, window, cx| file_editor::open_path(open.clone(), window, cx))
        });
        Some(
            v_flex()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .gap_2()
                        .pb_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(
                            Icon::new(IconName::Paperclip)
                                .xsmall()
                                .text_color(theme.muted_foreground),
                        )
                        .child("Files in this note"),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    fn render_empty(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("notes-empty")
            .m_auto()
            .items_center()
            .gap_2()
            .px_10()
            .py_10()
            .rounded_2xl()
            .cursor_pointer()
            .hover(|s| s.bg(theme.secondary))
            .child(
                Icon::new(IconName::StickyNote)
                    .large()
                    .text_color(theme.chart_1),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No notes yet"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Meeting notes, snippets, ideas — click or press ⌘⇧N."),
            )
            .on_click(cx.listener(|this, _, window, cx| this.create(window, cx)))
    }
}

impl Render for NotesPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let query = self.search.read(cx).value().trim().to_string();
        let all = self.store.read(cx).notes();
        let count = all.len();
        let visible: Vec<Note> = all.into_iter().filter(|n| n.matches(&query)).collect();
        let current = self
            .selected
            .and_then(|id| self.store.read(cx).note(id).cloned());
        let error: Option<SharedString> = self.store.read(cx).save_error.clone();
        let doc_w = if self.full { DOC_FULL_W } else { DOC_W };

        let document = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(self.render_toolbar(current.as_ref(), &theme, cx))
            .when_some(error, |this, err| {
                this.child(div().px_4().text_sm().text_color(theme.danger).child(err))
            })
            .child(match &current {
                Some(note) => div()
                    .id("note-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(doc_w))
                            .mx_auto()
                            .px_12()
                            .pt_6()
                            .pb_16()
                            .gap_4()
                            .child(
                                // Kit inputs pin their own height/text size; override for a
                                // document title flush with the body.
                                Input::new(&self.title)
                                    .appearance(false)
                                    .px_0()
                                    .h(px(46.))
                                    .text_size(px(32.))
                                    .line_height(px(42.))
                                    .font_weight(FontWeight::BOLD),
                            )
                            .child(self.render_properties(note, &theme, cx))
                            .child(self.render_body())
                            .children(self.render_files(note, &theme)),
                    )
                    .into_any_element(),
                None => v_flex()
                    .flex_1()
                    .child(self.render_empty(&theme, cx))
                    .into_any_element(),
            });

        h_flex()
            .size_full()
            .items_stretch()
            .gap_3()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.escape(window, cx)))
            .when(!self.full, |this| {
                this.child(self.render_list(&visible, count, &theme, cx))
            })
            .child(document)
    }
}
