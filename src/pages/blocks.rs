//! Custom blocks (`crate::blocks`): a list pane and an editor pane (settings, params,
//! environment, script, test run). Changes are saved explicitly, since every save bumps the
//! type's revision and so re-runs its blocks in notes.

use std::collections::BTreeMap;

use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    h_flex,
    input::{Editor, EditorState, Input, InputState, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    AnyElement, App, Context, Entity, FocusHandle, FontWeight, SharedString, Subscription, Task,
    Window, assets::IconName, div, prelude::*, px,
};

use crate::{
    blocks::{
        self, Blocks,
        model::{BlockType, FENCE, Output, Param, ParamKind, parse_config, valid_name},
        store, templates,
    },
    components::{
        confirm::{confirm, confirm_delete},
        icon_button, markdown_style, output_view, property_row,
    },
    storage,
};

const LIST_W: f32 = 260.;
const FORM_W: f32 = 760.;
const PILL: ButtonRounded = ButtonRounded::Size(px(999.));

pub struct BlocksPage {
    /// Saved block type being edited (`None` with a draft = a new, unsaved one).
    selected: Option<String>,
    draft: Option<Draft>,
    /// Config for test runs, as in a fence: `key: value` lines.
    test_config: Entity<TextareaState>,
    test: Option<Output>,
    testing: bool,
    next_row: u64,
    focus_handle: FocusHandle,
    _test: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// The form for one block type.
struct Draft {
    /// Distinguishes drafts, so each opens scrolled to the top.
    key: u64,
    is_new: bool,
    /// As saved (for `dirty`).
    base: BlockType,
    base_script: String,
    name: Entity<InputState>,
    label: Entity<InputState>,
    icon: Entity<InputState>,
    interpreter: Entity<InputState>,
    script_file: Entity<InputState>,
    ttl: Entity<InputState>,
    timeout: Entity<InputState>,
    params: Vec<ParamRow>,
    env: Vec<EnvRow>,
    script: Entity<EditorState>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

struct ParamRow {
    id: u64,
    name: Entity<InputState>,
    kind: ParamKind,
    default: Entity<InputState>,
}

struct EnvRow {
    id: u64,
    key: Entity<InputState>,
    value: Entity<InputState>,
}

fn kind_label(kind: ParamKind) -> &'static str {
    match kind {
        ParamKind::Text => "Text",
        ParamKind::Number => "Number",
        ParamKind::Bool => "Yes / no",
    }
}

fn value(state: &Entity<InputState>, cx: &App) -> String {
    state.read(cx).value().trim().to_string()
}

impl Draft {
    /// The block type as the form describes it.
    fn build(&self, cx: &App) -> BlockType {
        let number = |state: &Entity<InputState>, fallback: u64| {
            value(state, cx).parse().unwrap_or(fallback)
        };
        BlockType {
            name: if self.is_new {
                value(&self.name, cx)
            } else {
                self.base.name.clone()
            },
            label: value(&self.label, cx),
            icon: value(&self.icon, cx),
            interpreter: value(&self.interpreter, cx),
            script: value(&self.script_file, cx),
            ttl: number(&self.ttl, self.base.ttl),
            timeout: number(&self.timeout, self.base.timeout).max(1),
            params: self
                .params
                .iter()
                .filter(|p| !value(&p.name, cx).is_empty())
                .map(|p| Param {
                    name: value(&p.name, cx),
                    kind: p.kind,
                    default: value(&p.default, cx),
                })
                .collect(),
            env: self
                .env
                .iter()
                .filter(|e| !value(&e.key, cx).is_empty())
                .map(|e| (value(&e.key, cx), value(&e.value, cx)))
                .collect::<BTreeMap<_, _>>(),
            revision: self.base.revision,
        }
    }

    fn script(&self, cx: &App) -> String {
        self.script.read(cx).value().to_string()
    }

    fn dirty(&self, cx: &App) -> bool {
        self.is_new || self.build(cx) != self.base || self.script(cx) != self.base_script
    }
}

impl BlocksPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let test_config = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 12)
                .placeholder("key: value per line, as in a note")
        });
        let mut this = Self {
            selected: None,
            draft: None,
            test_config,
            test: None,
            testing: false,
            next_row: 0,
            focus_handle: cx.focus_handle(),
            _test: None,
            _subscriptions: vec![cx.observe_global::<Blocks>(|_, cx| cx.notify())],
        };
        if let Some(first) = blocks::types(cx).first().map(|t| t.name.clone()) {
            this.open(&first, window, cx);
        }
        this
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
    }

    fn field(
        &mut self,
        text: &str,
        placeholder: &'static str,
        subs: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let text = text.to_string();
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(text)
        });
        // Re-render on typing: Save's enabled state follows `dirty`.
        subs.push(cx.observe(&state, |_, _, cx| cx.notify()));
        state
    }

    fn param_row(
        &mut self,
        param: &Param,
        subs: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ParamRow {
        self.next_row += 1;
        ParamRow {
            id: self.next_row,
            name: self.field(&param.name, "name", subs, window, cx),
            kind: param.kind,
            default: self.field(&param.default, "default", subs, window, cx),
        }
    }

    fn env_row(
        &mut self,
        key: &str,
        val: &str,
        subs: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EnvRow {
        self.next_row += 1;
        EnvRow {
            id: self.next_row,
            key: self.field(key, "NAME", subs, window, cx),
            value: self.field(val, "value, or keychain:<service>", subs, window, cx),
        }
    }

    /// Shows `ty` in the form. `is_new`: not saved yet (the name is editable).
    fn load(
        &mut self,
        ty: BlockType,
        script: String,
        is_new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut subs = Vec::new();
        let name = self.field(&ty.name, "my-block", &mut subs, window, cx);
        let label = self.field(&ty.label, "Shown in the / menu", &mut subs, window, cx);
        let icon = self.field(
            &ty.icon,
            "Lucide name, e.g. list-checks",
            &mut subs,
            window,
            cx,
        );
        let interpreter = self.field(
            &ty.interpreter,
            "python3, node, bash… (empty = run the script itself)",
            &mut subs,
            window,
            cx,
        );
        let script_file = self.field(&ty.script, "run.py", &mut subs, window, cx);
        let ttl = self.field(&ty.ttl.to_string(), "300", &mut subs, window, cx);
        let timeout = self.field(&ty.timeout.to_string(), "30", &mut subs, window, cx);
        let params = ty
            .params
            .iter()
            .map(|p| self.param_row(p, &mut subs, window, cx))
            .collect();
        let env = ty
            .env
            .iter()
            .map(|(k, v)| self.env_row(k, v, &mut subs, window, cx))
            .collect();
        let script_state = {
            let text = script.clone();
            cx.new(|cx| EditorState::new(window, cx).default_value(text))
        };
        subs.push(cx.observe(&script_state, |_, _, cx| cx.notify()));
        let starter = ty.starter();
        self.test_config
            .update(cx, |t, cx| t.set_value(starter, window, cx));
        self.test = None;
        self.testing = false;
        self._test = None;
        self.selected = (!is_new).then(|| ty.name.clone());
        self.next_row += 1;
        self.draft = Some(Draft {
            key: self.next_row,
            is_new,
            base: ty,
            base_script: script,
            name,
            label,
            icon,
            interpreter,
            script_file,
            ttl,
            timeout,
            params,
            env,
            script: script_state,
            error: None,
            _subscriptions: subs,
        });
        cx.notify();
    }

    fn open(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ty) = blocks::find(name, cx).cloned() else {
            return;
        };
        let script = store::script(&ty);
        self.load(ty, script, false, window, cx);
    }

    /// Runs `then` now, or after "Discard changes?" when the form has unsaved edits.
    fn leave(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if !self.draft.as_ref().is_some_and(|d| d.dirty(cx)) {
            then(self, window, cx);
            return;
        }
        let this = cx.entity().downgrade();
        confirm(
            window,
            cx,
            "Discard changes?",
            "This block has unsaved changes.",
            "Discard",
            move |window, cx| {
                this.update(cx, |this, cx| then(this, window, cx)).ok();
            },
        );
    }

    /// A new, unsaved block from `templates::all()[template]`.
    fn create(&mut self, template: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.leave(window, cx, move |this, window, cx| {
            let Some(t) = templates::all().into_iter().nth(template) else {
                return;
            };
            let mut ty = t.ty;
            ty.name = t.name.to_string();
            let mut n = 2;
            while !ty.name.is_empty() && store::exists(&ty.name) {
                ty.name = format!("{}-{n}", t.name);
                n += 1;
            }
            this.load(ty, t.script.to_string(), true, window, cx);
            if let Some(d) = &this.draft {
                d.name.update(cx, |n, cx| n.focus(window, cx));
            }
        });
    }

    fn duplicate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &self.draft else { return };
        let mut ty = d.build(cx);
        let script = d.script(cx);
        let mut name = format!("{}-copy", ty.name);
        while store::exists(&name) {
            name.push_str("-copy");
        }
        ty.name = name;
        if !ty.label.is_empty() {
            ty.label.push_str(" copy");
        }
        ty.revision = 0;
        self.load(ty, script, true, window, cx);
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.selected.clone() else {
            return;
        };
        let this = cx.entity().downgrade();
        confirm_delete(
            window,
            cx,
            "Delete block?",
            format!(
                "“{name}” and its script will be deleted. Notes using it show “Unknown block type”."
            ),
            move |window, cx| {
                if let Err(err) = store::remove(&name) {
                    eprintln!("couldn't delete block {name}: {err:#}");
                }
                blocks::reload_types(cx);
                this.update(cx, |this, cx| {
                    this.draft = None;
                    this.selected = None;
                    if let Some(first) = blocks::types(cx).first().map(|t| t.name.clone()) {
                        this.open(&first, window, cx);
                    }
                    cx.notify();
                })
                .ok();
            },
        );
    }

    /// Writes the form to disk. False (with the reason shown) if it can't.
    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(d) = &mut self.draft else {
            return false;
        };
        let mut ty = d.build(cx);
        let script = d.script(cx);
        let problem = if !valid_name(&ty.name) {
            Some("Name: lowercase letters, digits, - and _ only")
        } else if d.is_new && store::exists(&ty.name) {
            Some("A block with this name already exists")
        } else if ty.script.is_empty() {
            Some("Script file needs a name")
        } else {
            None
        };
        if let Some(problem) = problem {
            d.error = Some(problem.into());
            cx.notify();
            return false;
        }
        if let Err(err) = store::save(&mut ty, &script) {
            d.error = Some(format!("Couldn't save: {err:#}").into());
            cx.notify();
            return false;
        }
        d.error = None;
        d.is_new = false;
        d.base = ty.clone();
        d.base_script = script;
        self.selected = Some(ty.name);
        blocks::reload_types(cx);
        cx.notify();
        true
    }

    /// Runs the saved block with the test config; saves first if there are changes.
    fn test_run(&mut self, cx: &mut Context<Self>) {
        if self.draft.as_ref().is_some_and(|d| d.dirty(cx)) && !self.save(cx) {
            return;
        }
        let Some(d) = &self.draft else { return };
        let config = parse_config(&self.test_config.read(cx).value());
        let rx = blocks::run_once(&d.base, &config);
        self.testing = true;
        self._test = Some(cx.spawn(async move |this, cx| {
            if let Ok(output) = rx.await {
                this.update(cx, |this, cx| {
                    this.test = Some(output);
                    this.testing = false;
                    cx.notify();
                })
                .ok();
            }
        }));
        cx.notify();
    }

    fn add_param(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut subs = Vec::new();
        let row = self.param_row(&Param::default(), &mut subs, window, cx);
        row.name.update(cx, |n, cx| n.focus(window, cx));
        if let Some(d) = &mut self.draft {
            d.params.push(row);
            d._subscriptions.extend(subs);
        }
        cx.notify();
    }

    fn add_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut subs = Vec::new();
        let row = self.env_row("", "", &mut subs, window, cx);
        row.key.update(cx, |n, cx| n.focus(window, cx));
        if let Some(d) = &mut self.draft {
            d.env.push(row);
            d._subscriptions.extend(subs);
        }
        cx.notify();
    }

    fn render_list(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let types = blocks::types(cx).to_vec();
        let new_draft = self.draft.as_ref().is_some_and(|d| d.is_new);
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
                    .child("Blocks"),
            )
            .when(!types.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(types.len().to_string()),
                )
            })
            .child(div().flex_1())
            .child(
                icon_button(
                    "new-block",
                    IconName::Plus,
                    theme.muted_foreground,
                    "New block",
                )
                .dropdown_menu({
                    let this = cx.entity().downgrade();
                    move |mut menu, _, _| {
                        menu = menu.label("New block from");
                        for (ix, t) in templates::all().into_iter().enumerate() {
                            let this = this.clone();
                            menu = menu.item(PopupMenuItem::new(t.label).on_click(
                                move |_, window, cx| {
                                    this.update(cx, |this, cx| this.create(ix, window, cx)).ok();
                                },
                            ));
                        }
                        menu
                    }
                }),
            );

        let row =
            |id: SharedString, icon: SharedString, label: String, name: String, selected: bool| {
                h_flex()
                    .id(id)
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_xl()
                    .cursor_pointer()
                    .when(selected, |s| s.bg(theme.secondary_active))
                    .when(!selected, |s| s.hover(|s| s.bg(theme.secondary)))
                    .child(
                        Icon::empty()
                            .path(icon)
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .truncate()
                                    .child(name),
                            ),
                    )
            };
        let rows = types.iter().map(|ty| {
            let name = ty.name.clone();
            let selected = !new_draft && self.selected.as_deref() == Some(&name);
            row(
                format!("block-row-{name}").into(),
                blocks::icon_path(&ty.icon, cx),
                ty.label().to_string(),
                format!("{FENCE} {name}"),
                selected,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.selected.as_deref() == Some(&name)
                    && !this.draft.as_ref().is_some_and(|d| d.is_new)
                {
                    return;
                }
                let name = name.clone();
                this.leave(window, cx, move |this, window, cx| {
                    this.open(&name, window, cx)
                });
            }))
        });
        let draft_row = new_draft.then(|| {
            row(
                "block-row-new".into(),
                IconName::Plus.path(),
                "New block".into(),
                "unsaved".into(),
                true,
            )
        });

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
            .child(
                div()
                    .id("blocks-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .child(v_flex().gap_0p5().children(draft_row).children(rows)),
            )
    }

    fn render_toolbar(
        &self,
        dirty: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let saved = self
            .selected
            .clone()
            .filter(|_| !self.draft.as_ref().is_some_and(|d| d.is_new));
        h_flex()
            .gap_1()
            .px_3()
            .pt_3()
            .child(div().flex_1())
            .when_some(saved, |this, name| {
                this.child(
                    icon_button(
                        "block-reveal",
                        IconName::FolderOpen,
                        theme.muted_foreground,
                        "Show in Finder",
                    )
                    .on_click(move |_, _, cx| {
                        cx.reveal_path(&storage::path(&store::folder(&name)))
                    }),
                )
                .child(
                    icon_button(
                        "block-duplicate",
                        IconName::CopyPlus,
                        theme.muted_foreground,
                        "Duplicate",
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.duplicate(window, cx))),
                )
                .child(
                    icon_button(
                        "block-delete",
                        IconName::Trash,
                        theme.muted_foreground,
                        "Delete block",
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.delete(window, cx))),
                )
            })
            .when(self.draft.is_some(), |this| {
                this.child(
                    Button::new("block-save")
                        .primary()
                        .small()
                        .rounded(PILL)
                        .label("Save")
                        .disabled(!dirty)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save(cx);
                        })),
                )
            })
    }

    fn section(title: &'static str, hint: Option<String>, theme: &Theme) -> gpui_kit::Div {
        v_flex().gap_2().child(
            h_flex()
                .gap_2()
                .items_baseline()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .children(
                    hint.map(|h| div().text_xs().text_color(theme.muted_foreground).child(h)),
                ),
        )
    }

    fn render_form(&self, d: &Draft, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let ty = d.build(cx);
        let input =
            |state: &Entity<InputState>| div().flex_1().min_w_0().child(Input::new(state).small());
        let title = if ty.label.is_empty() {
            if d.is_new {
                "New block".to_string()
            } else {
                ty.name.clone()
            }
        } else {
            ty.label.clone()
        };
        let usage = if valid_name(&ty.name) {
            format!(
                "In a note: type / and pick it, or write ```{FENCE} {}",
                ty.name
            )
        } else {
            "Give it a name to use it in notes".into()
        };

        let settings = v_flex()
            .gap_1()
            .when(d.is_new, |s| {
                s.child(property_row(IconName::Zap, "Name", theme).child(input(&d.name)))
            })
            .child(property_row(IconName::Type, "Label", theme).child(input(&d.label)))
            .child(
                property_row(IconName::Sparkles, "Icon", theme)
                    .child(
                        Icon::empty()
                            .path(blocks::icon_path(&ty.icon, cx))
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(input(&d.icon)),
            )
            .child(property_row(IconName::Play, "Runs with", theme).child(input(&d.interpreter)))
            .child(
                property_row(IconName::FileText, "Script file", theme).child(input(&d.script_file)),
            )
            .child(
                property_row(IconName::RotateCcw, "Fresh for (s)", theme)
                    .child(input(&d.ttl))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("0 = only on refresh"),
                    ),
            )
            .child(property_row(IconName::Timer, "Timeout (s)", theme).child(input(&d.timeout)));

        let this = cx.entity().downgrade();
        let params = Self::section(
            "Params",
            Some(
                "set per block in a note; passed as JSON on stdin and BLOCK_<NAME> env vars".into(),
            ),
            theme,
        )
        .children(d.params.iter().map(|row| {
            let id = row.id;
            let group: SharedString = format!("param-{id}").into();
            let kind_menu = {
                let this = this.clone();
                Button::new(("param-kind", id))
                    .ghost()
                    .small()
                    .rounded(PILL)
                    .label(kind_label(row.kind))
                    .dropdown_menu(move |mut menu, _, _| {
                        for kind in [ParamKind::Text, ParamKind::Number, ParamKind::Bool] {
                            let this = this.clone();
                            menu =
                                menu.item(PopupMenuItem::new(kind_label(kind)).on_click(
                                    move |_, _, cx| {
                                        this.update(cx, |this, cx| {
                                            if let Some(row) = this.draft.as_mut().and_then(|d| {
                                                d.params.iter_mut().find(|r| r.id == id)
                                            }) {
                                                row.kind = kind;
                                            }
                                            cx.notify();
                                        })
                                        .ok();
                                    },
                                ));
                        }
                        menu
                    })
            };
            h_flex()
                .group(group.clone())
                .gap_2()
                .items_center()
                .child(div().w(px(180.)).child(Input::new(&row.name).small()))
                .child(div().w(px(110.)).child(kind_menu))
                .child(input(&row.default))
                .child(
                    div()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            icon_button(
                                ("param-remove", id),
                                IconName::X,
                                theme.muted_foreground,
                                "Remove",
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(d) = &mut this.draft {
                                        d.params.retain(|r| r.id != id);
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                )
        }))
        .child(
            h_flex().child(
                Button::new("param-add")
                    .ghost()
                    .small()
                    .rounded(PILL)
                    .icon(Icon::new(IconName::Plus).text_color(theme.muted_foreground))
                    .label("Add param")
                    .on_click(cx.listener(|this, _, window, cx| this.add_param(window, cx))),
            ),
        );

        let env = Self::section(
            "Environment",
            Some("keychain:<service> reads a secret from your login keychain at run time".into()),
            theme,
        )
        .children(d.env.iter().map(|row| {
            let id = row.id;
            let group: SharedString = format!("env-{id}").into();
            h_flex()
                .group(group.clone())
                .gap_2()
                .items_center()
                .child(div().w(px(180.)).child(Input::new(&row.key).small()))
                .child(input(&row.value))
                .child(
                    div()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            icon_button(
                                ("env-remove", id),
                                IconName::X,
                                theme.muted_foreground,
                                "Remove",
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(d) = &mut this.draft {
                                        d.env.retain(|r| r.id != id);
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                )
        }))
        .child(
            h_flex().child(
                Button::new("env-add")
                    .ghost()
                    .small()
                    .rounded(PILL)
                    .icon(Icon::new(IconName::Plus).text_color(theme.muted_foreground))
                    .label("Add variable")
                    .on_click(cx.listener(|this, _, window, cx| this.add_env(window, cx))),
            ),
        );

        let command = if ty.interpreter.is_empty() {
            format!("./{}", ty.script)
        } else {
            format!("{} {}", ty.interpreter, ty.script)
        };
        let script = Self::section(
            "Script",
            Some(format!("{command} — print Markdown to stdout")),
            theme,
        )
        .child(
            div()
                .rounded_xl()
                .overflow_hidden()
                .child(Editor::new(&d.script).h(px(360.)).text_sm()),
        );

        v_flex()
            .w_full()
            .max_w(px(FORM_W))
            .mx_auto()
            .px_12()
            .pt_4()
            .pb_16()
            .gap_8()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                Icon::empty()
                                    .path(blocks::icon_path(&ty.icon, cx))
                                    .large()
                                    .text_color(theme.chart_1),
                            )
                            .child(div().text_2xl().font_weight(FontWeight::BOLD).child(title)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(usage),
                    )
                    .children(
                        d.error
                            .clone()
                            .map(|err| div().pt_1().text_sm().text_color(theme.danger).child(err)),
                    ),
            )
            .child(settings)
            .child(params)
            .child(env)
            .child(script)
            .child(self.render_test(d.dirty(cx), theme, cx))
    }

    fn render_test(&self, dirty: bool, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let run = Button::new("block-test")
            .small()
            .rounded(PILL)
            .icon(Icon::new(IconName::Play))
            .label(match (self.testing, dirty) {
                (true, _) => "Running…",
                (false, true) => "Save & run",
                (false, false) => "Run",
            })
            .disabled(self.testing)
            .on_click(cx.listener(|this, _, _, cx| this.test_run(cx)));
        let output = self.test.as_ref().map(|out| {
            let status = format!(
                "{} · {} ms",
                if out.ok { "Succeeded" } else { "Failed" },
                out.duration_ms
            );
            v_flex()
                .gap_2()
                .p_4()
                .rounded_2xl()
                .border_1()
                .border_color(theme.border)
                .bg(theme.group_box)
                .child(
                    div()
                        .text_xs()
                        .text_color(if out.ok {
                            theme.muted_foreground
                        } else {
                            theme.danger
                        })
                        .child(status),
                )
                .when(!out.markdown.trim().is_empty(), |d| {
                    d.child(output_view(
                        "block-test-output",
                        out.markdown.clone(),
                        markdown_style(),
                        storage::path(&store::folder(&self.selected.clone().unwrap_or_default())),
                    ))
                })
                .when(!out.stderr.is_empty(), |d| {
                    d.child(
                        div()
                            .text_xs()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.muted_foreground)
                            .whitespace_normal()
                            .child(out.stderr.clone()),
                    )
                })
        });
        Self::section(
            "Test run",
            Some("config as in a note's block".into()),
            theme,
        )
        .child(
            div()
                .px_3()
                .py_2()
                .rounded_xl()
                .bg(theme.secondary)
                .font_family(theme.mono_font_family.clone())
                .child(Textarea::new(&self.test_config).appearance(false).text_sm()),
        )
        .child(h_flex().child(run))
        .children(output)
    }

    fn render_empty(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("blocks-empty")
            .m_auto()
            .items_center()
            .gap_2()
            .px_10()
            .py_10()
            .rounded_2xl()
            .cursor_pointer()
            .hover(|s| s.bg(theme.secondary))
            .child(
                Icon::new(IconName::Blocks)
                    .large()
                    .text_color(theme.chart_1),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No custom blocks yet"),
            )
            .child(
                div()
                    .max_w(px(360.))
                    .text_xs()
                    .text_center()
                    .text_color(theme.muted_foreground)
                    .child(
                        "A block runs a script and shows its Markdown in your notes — \
                         a Jira query, a report, anything. Click to make one.",
                    ),
            )
            .on_click(cx.listener(|this, _, window, cx| this.create(0, window, cx)))
    }
}

impl Render for BlocksPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dirty = self.draft.as_ref().is_some_and(|d| d.dirty(cx));
        let body: AnyElement = match &self.draft {
            Some(d) => div()
                .id(("block-scroll", d.key))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(self.render_form(d, &theme, cx))
                .into_any_element(),
            None => v_flex()
                .flex_1()
                .child(self.render_empty(&theme, cx))
                .into_any_element(),
        };
        let editor = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(self.render_toolbar(dirty, &theme, cx))
            .child(body);

        h_flex()
            .size_full()
            .items_stretch()
            .gap_3()
            .track_focus(&self.focus_handle)
            .child(self.render_list(&theme, cx))
            .child(editor)
    }
}
