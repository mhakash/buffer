//! Standalone Markdown notes, Notion-style: a list pane and a document pane that fill the
//! content area. The title is always editable; the body is rendered Markdown until you edit
//! it. Full view hides the list.

use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Size, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    h_flex,
    input::{Escape, Input, InputEvent, InputState, Textarea, TextareaState},
    text::TextView,
    v_flex,
};
use gpui_kit::{
    AnyElement, Context, Entity, FocusHandle, FontWeight, SharedString, Subscription, Window,
    assets::IconName, div, prelude::*, px,
};

use super::task_detail::when;
use crate::{
    components::{confirm::confirm_delete, document_style, icon_button},
    notes::{Note, NoteId, NoteStore},
};

const LIST_W: f32 = 272.;
/// Reading width of the document; full view allows a little more.
const DOC_W: f32 = 720.;
const DOC_FULL_W: f32 = 860.;

pub struct NotesPage {
    store: Entity<NoteStore>,
    selected: Option<NoteId>,
    /// Body shows the Markdown source instead of the rendered note.
    editing: bool,
    /// List hidden, document takes the whole area.
    full: bool,
    title: Entity<InputState>,
    editor: Entity<TextareaState>,
    search: Entity<InputState>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl NotesPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(NoteStore::load);
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled"));
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(12, 5000)
                .placeholder("Write in Markdown: # headings, - [ ] checklists, `code`, links…")
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscriptions = vec![
            // Keep a selection whenever there's a note (first launch, after a delete).
            cx.observe_in(&store, window, |this, _, window, cx| {
                this.ensure_selection(window, cx);
                cx.notify();
            }),
            cx.observe(&search, |_, _, cx| cx.notify()),
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
                        InputEvent::PressEnter { .. } => this.edit(window, cx),
                        _ => {}
                    }
                },
            ),
            cx.subscribe_in(&editor, window, |this, input, event: &InputEvent, _, cx| {
                if let (InputEvent::Change, Some(id), true) = (event, this.selected, this.editing) {
                    let value = input.read(cx).value();
                    this.store.update(cx, |s, cx| s.set_content(id, &value, cx));
                }
            }),
        ];
        let mut this = Self {
            store,
            selected: None,
            editing: false,
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
        if self.editing {
            self.editor.update(cx, |e, cx| e.focus(window, cx));
        } else {
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
        self.editing = false;
        let title = self
            .store
            .read(cx)
            .note(id)
            .map(|n| n.split().title.to_string())
            .unwrap_or_default();
        self.title
            .update(cx, |t, cx| t.set_value(title, window, cx));
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
                self.editing = false;
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
        // The store is the source of truth while previewing (checkbox clicks edit it there).
        self.editing = true;
        self.editor.update(cx, |e, cx| {
            e.set_value(content, window, cx);
            e.focus(window, cx);
        });
        cx.notify();
    }

    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Esc: leave the editor, then full view.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing {
            self.preview(window, cx);
        } else if self.full {
            self.full = false;
            cx.notify();
        }
    }

    /// A note left blank isn't worth keeping: leaving it removes it.
    fn drop_if_empty(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected
            && self
                .store
                .read(cx)
                .note(id)
                .is_some_and(|n| n.text.trim().is_empty())
        {
            self.selected = None;
            self.editing = false;
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
        let editing = self.editing;
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
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .mr_1()
                        .child(format!("Edited {}", when(note.updated_at))),
                )
                .child(
                    Button::new("note-mode")
                        .ghost()
                        .small()
                        .rounded(ButtonRounded::Size(px(999.)))
                        .icon(
                            Icon::new(if editing {
                                IconName::Eye
                            } else {
                                IconName::SquarePen
                            })
                            .text_color(theme.muted_foreground),
                        )
                        .label(if editing { "Preview" } else { "Edit" })
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.editing {
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

    fn render_body(&self, note: &Note, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = note.id;
        let parts = note.split();
        if self.editing {
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
        // Clicking the blank space under the note continues writing, like a document.
        let write_here = |id: &'static str, min_h: f32| {
            div()
                .id(id)
                .w_full()
                .min_h(px(min_h))
                .cursor_text()
                .on_click(cx.listener(|this, _, window, cx| this.edit(window, cx)))
        };
        if parts.content.trim().is_empty() {
            return write_here("note-empty", 240.)
                .text_base()
                .text_color(theme.muted_foreground)
                .child("Write something — Markdown works: # headings, - [ ] checklists, `code`…")
                .into_any_element();
        }
        let offset = parts.content_offset(&note.text);
        let store = self.store.clone();
        v_flex()
            .text_base()
            .child(
                TextView::markdown(("note-md", id), parts.content.to_string())
                    .style(document_style())
                    .selectable(true)
                    .on_task_toggle(move |event, _, cx| {
                        let marker = event.marker_range();
                        let marker = marker.start + offset..marker.end + offset;
                        store.update(cx, |s, cx| s.set_checkbox(id, marker, event.checked(), cx));
                    }),
            )
            .child(write_here("note-tail", 160.))
            .into_any_element()
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
                            .child(self.render_body(note, &theme, cx)),
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
