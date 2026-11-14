//! Standalone Markdown notes: searchable list on the left, the selected note on the right
//! (rendered Markdown, or the editor while editing).

use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Theme,
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
    components::{confirm::confirm_delete, icon_button, markdown_style},
    notes::{Note, NoteId, NoteStore},
};

const LIST_W: f32 = 260.;

pub struct NotesPage {
    store: Entity<NoteStore>,
    selected: Option<NoteId>,
    editing: bool,
    editor: Entity<TextareaState>,
    search: Entity<InputState>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl NotesPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(NoteStore::load);
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(16, 2000)
                .placeholder("Title on the first line, then Markdown…")
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes"));
        let selected = store.read(cx).notes().first().map(|n| n.id);
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.observe(&search, |_, _, cx| cx.notify()),
            cx.subscribe_in(&editor, window, |this, input, event: &InputEvent, _, cx| {
                if let (InputEvent::Change, Some(id), true) = (event, this.selected, this.editing) {
                    let value = input.read(cx).value();
                    this.store.update(cx, |s, cx| s.set_body(id, &value, cx));
                }
            }),
        ];
        Self {
            store,
            selected,
            editing: false,
            editor,
            search,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
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
        self.selected = Some(id);
        self.edit(window, cx);
    }

    fn select(&mut self, id: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            return;
        }
        self.drop_if_empty(cx);
        self.selected = Some(id);
        self.editing = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self
            .selected
            .and_then(|id| self.store.read(cx).note(id).map(|n| n.body.clone()))
        else {
            return;
        };
        // The store is the source of truth while previewing (checkbox clicks edit it there).
        self.editing = true;
        self.editor.update(cx, |e, cx| {
            e.set_value(body, window, cx);
            e.focus(window, cx);
        });
        cx.notify();
    }

    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// A note left blank isn't worth keeping: leaving it removes it.
    fn drop_if_empty(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected
            && self
                .store
                .read(cx)
                .note(id)
                .is_some_and(|n| n.body.trim().is_empty())
        {
            self.store.update(cx, |s, cx| s.delete(id, cx));
            self.selected = None;
            self.editing = false;
        }
    }

    fn render_header(
        &self,
        count: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .justify_between()
            .items_end()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Notes"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(match count {
                                0 => "Anything worth keeping, in Markdown.".to_string(),
                                1 => "1 note".to_string(),
                                n => format!("{n} notes"),
                            }),
                    ),
            )
            .child(
                Button::new("new-note")
                    .primary()
                    .small()
                    .rounded(ButtonRounded::Size(px(999.)))
                    .icon(IconName::Plus)
                    .label("New note")
                    .tooltip("New note (⌘⇧N)")
                    .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
            )
    }

    fn render_list(
        &self,
        notes: &[Note],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let search = h_flex()
            .gap_2()
            .px_3()
            .mb_1()
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

        let query_empty = self.search.read(cx).value().is_empty();
        v_flex()
            .flex_none()
            .w(px(LIST_W))
            .gap_0p5()
            .child(search)
            .children(rows)
            .when(notes.is_empty() && !query_empty, |this| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("No matching notes"),
                )
            })
    }

    fn render_note(&self, note: &Note, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = note.id;
        let editing = self.editing;
        let toolbar = h_flex()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Edited {}", when(note.updated_at))),
            )
            .child(
                Button::new("note-mode")
                    .ghost()
                    .xsmall()
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
                .on_click({
                    let title = note.title();
                    cx.listener(move |this, _, window, cx| {
                        let store = this.store.clone();
                        confirm_delete(
                            window,
                            cx,
                            "Delete note?",
                            format!("“{title}” will be deleted."),
                            move |_, cx| store.update(cx, |s, cx| s.delete(id, cx)),
                        );
                    })
                }),
            );

        let body = if editing {
            div()
                .text_sm()
                .font_family(theme.mono_font_family.clone())
                .child(Textarea::new(&self.editor).appearance(false))
                .into_any_element()
        } else if note.body.trim().is_empty() {
            div()
                .id("note-empty")
                .py_2()
                .cursor_text()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Empty note — click to write.")
                .on_click(cx.listener(|this, _, window, cx| this.edit(window, cx)))
                .into_any_element()
        } else {
            let store = self.store.clone();
            div()
                .text_sm()
                .child(
                    TextView::markdown(("note-md", id), note.body.clone())
                        .style(markdown_style())
                        .selectable(true)
                        .on_task_toggle(move |event, _, cx| {
                            store.update(cx, |s, cx| {
                                s.set_checkbox(id, event.marker_range(), event.checked(), cx)
                            });
                        }),
                )
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .px_6()
            .pt_3()
            .pb_6()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(if editing {
                theme.primary.opacity(0.35)
            } else {
                theme.border
            })
            .child(toolbar)
            .child(body)
            .into_any_element()
    }

    fn render_empty(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("notes-empty")
            .flex_1()
            .items_center()
            .gap_2()
            .py_16()
            .rounded_2xl()
            .cursor_pointer()
            .border_1()
            .border_dashed()
            .border_color(theme.border)
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
        // Keep a selection while there's something to show (e.g. after a delete).
        if self
            .selected
            .is_none_or(|id| self.store.read(cx).note(id).is_none())
        {
            self.selected = visible.first().map(|n| n.id);
            self.editing = false;
        }
        let current = self
            .selected
            .and_then(|id| self.store.read(cx).note(id).cloned());
        let error = self.store.read(cx).save_error.clone();

        v_flex()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                if this.editing {
                    this.preview(window, cx);
                }
            }))
            .gap_5()
            .child(self.render_header(count, &theme, cx))
            .when_some(error, |this, err: SharedString| {
                this.child(div().text_sm().text_color(theme.danger).child(err))
            })
            .child(if count == 0 {
                self.render_empty(&theme, cx).into_any_element()
            } else {
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(self.render_list(&visible, &theme, cx))
                    .children(current.map(|note| self.render_note(&note, &theme, cx)))
                    .into_any_element()
            })
    }
}
