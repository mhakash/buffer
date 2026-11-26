//! Controls a block script prints in its output: an ```` ```input ```` fence becomes a row of
//! text and number fields, toggles, dropdowns and buttons (`blocks::model::Control`). A changed
//! value is written into the note's fence through the block's [`BlockHost`], which re-runs the
//! block with it; a button runs the block once with `action: <name>`.
//!
//! Text fields keep their `InputState` in the [`Controls`] global, per block instance and name,
//! so typing survives re-renders and re-runs. Text and number commit on Enter or blur; toggles
//! and dropdowns at once.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonRounded, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    switch::Switch,
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView, markdown_ast},
};
use gpui_kit::{
    AnyElement, App, AppContext, Entity, Focusable, Global, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Styled, Subscription, Window, div, prelude::FluentBuilder as _, px,
};

use crate::blocks::{
    self,
    model::{Config, Control, ControlKind, INPUT_FENCE, Output, parse_controls},
};

/// Writes `name: value` into the block's fence.
pub type SetParam = Arc<dyn Fn(&str, &str, &mut App) + Send + Sync>;

/// The editor a block is shown in: controls write their values back through it.
#[derive(Clone)]
pub struct BlockHost {
    /// Unique among the blocks on screen.
    pub id: u64,
    pub set: SetParam,
}

/// What the controls of one block's output act on; rebuilt every frame.
#[derive(Clone)]
pub(super) struct ControlCtx {
    /// Block instance: the host's id, else one derived from the view.
    pub id: u64,
    pub ty: String,
    pub config: Config,
    pub host: Option<BlockHost>,
    /// Scripts may run (and there is a host to write to).
    pub enabled: bool,
    pub running: bool,
}

pub(super) struct ControlsPlugin(pub ControlCtx);

/// Controls in a block's output shown but inert (the Blocks page's test run).
pub fn preview_controls(view: TextView) -> TextView {
    view.plugin(ControlsPlugin(ControlCtx {
        id: 0,
        ty: String::new(),
        config: Config::new(),
        host: None,
        enabled: false,
        running: false,
    }))
}

struct Field {
    input: Entity<InputState>,
    set: Option<SetParam>,
    /// The value in the fence; committing it again is a no-op.
    committed: String,
    _subscription: Subscription,
}

#[derive(Default)]
struct Controls {
    fields: HashMap<(u64, String), Field>,
    /// Last output each block instance showed, kept while a new value runs.
    shown: HashMap<u64, (String, Output)>,
}

impl Global for Controls {}

/// The output whose Markdown a hosted block shows: its own, else (a new value still running, or
/// failed with nothing) the one it showed last, so its controls stay usable.
pub(super) fn shown_output(
    host: Option<&BlockHost>,
    ty: &str,
    output: Option<Output>,
    cx: &mut App,
) -> Option<Output> {
    let Some(host) = host else { return output };
    let shown = &mut cx.default_global::<Controls>().shown;
    match output {
        Some(out) if !out.markdown.trim().is_empty() => {
            shown.insert(host.id, (ty.to_string(), out.clone()));
            Some(out)
        }
        output => shown
            .get(&host.id)
            .filter(|(t, _)| t == ty)
            .map(|(_, out)| out.clone())
            .or(output),
    }
}

impl MarkdownPlugin for ControlsPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        INPUT_FENCE
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        _: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else {
            return None;
        };
        (code.lang.as_deref() == Some(INPUT_FENCE))
            .then(|| MarkdownNode::new(INPUT_FENCE, parse_controls(&code.value)))
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = &self.0;
        let controls = node.data::<Vec<Control>>().cloned().unwrap_or_default();
        h_flex()
            .id(("blk-controls", ctx.id))
            .w_full()
            .my_1()
            .flex_wrap()
            .gap_2()
            .items_center()
            // Presses here belong to the controls: not a text selection, nor (via the click
            // that would follow) "edit this block" in the note editor.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(
                controls
                    .iter()
                    .map(|control| render_control(control, ctx, window, cx)),
            )
    }
}

fn render_control(
    control: &Control,
    ctx: &ControlCtx,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.theme().clone();
    let id = element_id(ctx.id, control);
    let value = control.value(&ctx.config);
    let set = {
        let (host, name) = (ctx.host.clone(), control.name.clone());
        move |value: &str, cx: &mut App| {
            if let Some(host) = &host {
                (host.set)(&name, value, cx);
            }
        }
    };
    let label = |text: &str| {
        div()
            .flex_none()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text.to_string())
    };
    match control.kind {
        ControlKind::Text | ControlKind::Number => {
            let input = field(control, ctx, &value, window, cx);
            h_flex()
                .gap_1p5()
                .items_center()
                .when(!control.label.is_empty(), |d| {
                    d.child(label(&control.label))
                })
                .child(
                    div()
                        .w(px(if control.kind == ControlKind::Number {
                            96.
                        } else {
                            220.
                        }))
                        .child(Input::new(&input).small().disabled(!ctx.enabled)),
                )
                .into_any_element()
        }
        ControlKind::Toggle => Switch::new(("blk-ctl", id))
            .small()
            .checked(control.checked(&ctx.config))
            .label(control.title().to_string())
            .disabled(!ctx.enabled)
            .on_click(move |checked: &bool, _, cx| {
                cx.stop_propagation();
                set(if *checked { "true" } else { "false" }, cx);
            })
            .into_any_element(),
        ControlKind::Select => {
            let options = control.options.clone();
            let current = value.clone();
            Button::new(("blk-ctl", id))
                .ghost()
                .small()
                .rounded(ButtonRounded::Size(px(999.)))
                .disabled(!ctx.enabled || options.is_empty())
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_sm()
                        .when(!control.label.is_empty(), |d| {
                            d.child(label(&control.label))
                        })
                        .child(value)
                        .child(
                            Icon::new(IconName::ChevronDown)
                                .xsmall()
                                .text_color(theme.muted_foreground),
                        ),
                )
                .dropdown_menu(move |mut menu, _, _| {
                    for option in &options {
                        let (set, option) = (set.clone(), option.clone());
                        menu = menu.item(
                            PopupMenuItem::new(option.clone())
                                .checked(option == current)
                                .on_click(move |_, _, cx| set(&option, cx)),
                        );
                    }
                    menu
                })
                .into_any_element()
        }
        ControlKind::Button => {
            let (ty, config, action) = (ctx.ty.clone(), ctx.config.clone(), control.name.clone());
            Button::new(("blk-ctl", id))
                .small()
                .rounded(ButtonRounded::Size(px(999.)))
                .label(control.title().to_string())
                .disabled(!ctx.enabled || ctx.running)
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if let Some(ty) = blocks::find(&ty, cx).cloned() {
                        blocks::run_action(&ty, &config, &action, cx);
                    }
                })
                .into_any_element()
        }
    }
}

/// The text field for `control` in this block instance, created on first use and kept in sync
/// with the fence while it isn't being typed in.
fn field(
    control: &Control,
    ctx: &ControlCtx,
    value: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let key = (ctx.id, control.name.clone());
    let existing = cx
        .default_global::<Controls>()
        .fields
        .get(&key)
        .map(|f| f.input.clone());
    let input = match existing {
        Some(input) => {
            let typing = input.focus_handle(cx).is_focused(window);
            if !typing && input.read(cx).value().as_ref() != value {
                input.update(cx, |i, cx| i.set_value(value.to_string(), window, cx));
            }
            input
        }
        None => {
            let placeholder = [&control.placeholder, &control.label, &control.name]
                .into_iter()
                .find(|s| !s.is_empty())
                .cloned()
                .unwrap_or_default();
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value.to_string())
            });
            let subscription = cx.subscribe(&input, {
                let key = key.clone();
                move |input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        commit(&key, input.read(cx).value().trim().to_string(), cx);
                    }
                }
            });
            cx.default_global::<Controls>().fields.insert(
                key.clone(),
                Field {
                    input: input.clone(),
                    set: None,
                    committed: String::new(),
                    _subscription: subscription,
                },
            );
            input
        }
    };
    if let Some(field) = cx.default_global::<Controls>().fields.get_mut(&key) {
        field.set = ctx.host.as_ref().map(|h| h.set.clone());
        field.committed = value.to_string();
    }
    input
}

fn commit(key: &(u64, String), value: String, cx: &mut App) {
    let Some(field) = cx.default_global::<Controls>().fields.get_mut(key) else {
        return;
    };
    if field.committed == value {
        return;
    }
    let Some(set) = field.set.clone() else { return };
    field.committed = value.clone();
    set(&key.1, &value, cx);
}

fn element_id(instance: u64, control: &Control) -> u64 {
    let mut h = DefaultHasher::new();
    (instance, &control.name, control.kind as u8).hash(&mut h);
    h.finish()
}
