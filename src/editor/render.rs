//! How blocks look. The active block and its rendered form share text metrics so swapping
//! between them doesn't move the text. Each row reaches left into the gutter (so hovering the
//! handles keeps them shown) and records its bounds for pointer hit-testing.

use gpui_kit::component::{
    Icon, Sizable, Size, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::Textarea,
    v_flex,
};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, Hsla, MouseButton, MouseDownEvent,
    assets::IconName, canvas, div, prelude::*, px, rems,
};

use super::{
    BlockEditor,
    model::{Kind, inline_markdown},
};
use crate::blocks::{self, model::ParamKind};
use crate::components::{document_style, markdown_style, markdown_view, take_claimed_click};

const INDENT: f32 = 24.;
const MARKER_W: f32 = 24.;

/// Text size, weight and line height (px) per kind.
fn metrics(kind: &Kind, compact: bool) -> (f32, FontWeight, f32) {
    if compact {
        return match kind {
            Kind::Heading(1) => (18., FontWeight::BOLD, 26.),
            Kind::Heading(2) => (16., FontWeight::BOLD, 24.),
            Kind::Heading(_) => (15., FontWeight::SEMIBOLD, 22.),
            Kind::Code(_) | Kind::Raw => (13., FontWeight::NORMAL, 19.),
            _ => (14., FontWeight::NORMAL, 22.),
        };
    }
    match kind {
        Kind::Heading(1) => (26., FontWeight::BOLD, 34.),
        Kind::Heading(2) => (21., FontWeight::BOLD, 29.),
        Kind::Heading(_) => (18., FontWeight::SEMIBOLD, 26.),
        Kind::Code(_) | Kind::Raw => (14., FontWeight::NORMAL, 21.),
        _ => (16., FontWeight::NORMAL, 26.),
    }
}

/// Space above a block, so headings start sections.
fn space_above(kind: &Kind, first: bool, compact: bool) -> f32 {
    let space = match kind {
        _ if first => 0.,
        Kind::Heading(1) => 20.,
        Kind::Heading(2) => 14.,
        Kind::Heading(_) => 10.,
        Kind::Code(_) | Kind::Raw | Kind::Divider | Kind::Table => 6.,
        _ => 1.,
    };
    if compact {
        (space * 0.6_f32).round()
    } else {
        space
    }
}

impl BlockEditor {
    pub(super) fn render_block(
        &self,
        ix: usize,
        number: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let block = &self.doc.blocks[ix];
        let id = block.id;
        let kind = &block.kind;
        let compact = self.compact;
        let (_, _, line_h) = metrics(kind, compact);
        let above = space_above(kind, ix == 0, compact);
        let gutter = self.gutter();
        let indent_x = block.indent as f32 * INDENT;
        let selected = self.is_selected(ix);
        let bounds = self.row_bounds.clone();

        let row = h_flex()
            .id(("blk-row", id))
            .group(format!("blk-{id}"))
            .relative()
            .items_start()
            .pt(px(above))
            .ml(px(-gutter))
            .pl(px(gutter + indent_x))
            .when(self.is_dragged(id), |r| r.opacity(0.4))
            .child(
                canvas(
                    move |b, _, _| {
                        if let Some(slot) = bounds.borrow_mut().get_mut(ix) {
                            *slot = b;
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(self.drop_at == Some(ix), |r| {
                r.child(self.drop_line(gutter + indent_x, theme))
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if let Some(ix) = this.index_of(id) {
                        this.open_block_menu(ix, event.position, None, window, cx);
                    }
                }),
            );

        let content = if *kind == Kind::Divider {
            h_flex()
                .flex_1()
                .h(px(25.))
                .items_center()
                .child(div().flex_1().h(px(1.)).bg(theme.border))
        } else {
            h_flex()
                .flex_1()
                .min_w_0()
                .items_start()
                .when(*kind == Kind::Quote, |d| {
                    d.pl_3()
                        .border_l_2()
                        .border_color(Hsla::from(theme.foreground).opacity(0.7))
                })
                .children(self.render_marker(kind, id, number, line_h, theme, cx))
                .child(self.render_body(ix, theme, cx))
        }
        .rounded_md()
        .when(selected, |d| d.bg(theme.primary.opacity(0.14)));

        let handle_top = above
            + match kind {
                Kind::Divider => 2.,
                k if !k.is_text() => 4.,
                _ => (line_h - 22.) / 2.,
            };
        let active = self.active == Some(ix);
        row.child(content)
            .child(self.render_handles(ix, indent_x, handle_top, theme, cx))
            .when(!active && *kind != Kind::Divider, |row| {
                row.on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    // A click in a custom block's output selects text or follows a link.
                    if this.drag_selecting || take_claimed_click(event.position()) {
                        return;
                    }
                    if let Some(ix) = this.index_of(id) {
                        this.click_block(ix, event.position(), window, cx);
                    }
                }))
            })
            .into_any_element()
    }

    /// The block's text: the textarea when active, else rendered Markdown.
    fn render_body(&self, ix: usize, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let block = &self.doc.blocks[ix];
        if block.kind == Kind::Table {
            return self.render_table(ix, theme, cx);
        }
        let id = block.id;
        let kind = &block.kind;
        let compact = self.compact;
        let (size, weight, line_h) = metrics(kind, compact);
        let code_like = !kind.is_text();
        let style = || {
            if compact {
                markdown_style()
            } else {
                document_style()
            }
        };

        if self.active == Some(ix) {
            return v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .relative()
                        .when(code_like, |d| {
                            d.px_3().py_2().rounded_xl().bg(theme.secondary)
                        })
                        .child(
                            Textarea::new(&self.input)
                                .appearance(false)
                                .with_size(Size::XSmall)
                                .ml(px(-4.))
                                .text_size(px(size))
                                .line_height(px(line_h))
                                .font_weight(weight)
                                .when(code_like, |t| t.font_family(theme.mono_font_family.clone())),
                        ),
                )
                .children(block_hint(kind, theme, cx))
                .when(self.slash.is_some(), |this| {
                    this.child(self.render_slash(theme, cx))
                })
                .into_any_element();
        }
        if code_like {
            return div()
                .flex_1()
                .min_w_0()
                .cursor_text()
                .text_size(px(if compact { 14. } else { 16. }))
                .child(
                    markdown_view(("blk-md", id), block.source(), style(), self.dir.clone())
                        .selectable(false),
                )
                .into_any_element();
        }
        if block.text.trim().is_empty() {
            return div()
                .flex_1()
                .h(px(line_h))
                .cursor_text()
                .into_any_element();
        }
        div()
            .flex_1()
            .min_w_0()
            .cursor_text()
            .text_size(px(size))
            .line_height(px(line_h))
            .font_weight(weight)
            .when(matches!(kind, Kind::Todo(true)), |d| {
                d.text_color(theme.muted_foreground).line_through()
            })
            .child(
                markdown_view(
                    ("blk-md", id),
                    inline_markdown(&block.text),
                    style().paragraph_gap(rems(0.)),
                    self.dir.clone(),
                )
                .selectable(false),
            )
            .into_any_element()
    }

    fn render_marker(
        &self,
        kind: &Kind,
        id: u64,
        number: usize,
        line_h: f32,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let cell = || {
            h_flex()
                .flex_none()
                .w(px(MARKER_W))
                .h(px(line_h))
                .items_center()
        };
        Some(match kind {
            Kind::Bullet => cell()
                .justify_center()
                .child(div().size(px(5.)).rounded_full().bg(theme.foreground))
                .into_any_element(),
            Kind::Numbered => cell()
                .justify_end()
                .pr_1p5()
                .child(format!("{number}."))
                .into_any_element(),
            Kind::Todo(done) => cell()
                .child(
                    Checkbox::new(("blk-todo", id))
                        .checked(*done)
                        .on_click(cx.listener(move |this, _: &bool, _, cx| {
                            cx.stop_propagation();
                            this.toggle_todo(id, cx);
                        })),
                )
                .into_any_element(),
            _ => return None,
        })
    }

    /// "+" (add below) and the drag handle, shown on hover (and on selected blocks).
    fn render_handles(
        &self,
        ix: usize,
        left: f32,
        top: f32,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = self.doc.blocks[ix].id;
        let add = Button::new(("blk-add", id))
            .ghost()
            .with_size(if self.compact {
                Size::Size(px(self.handle_size()))
            } else {
                Size::XSmall
            })
            .rounded(ButtonRounded::Size(px(6.)))
            .icon(Icon::new(IconName::Plus).text_color(theme.muted_foreground))
            .tooltip("Add a block below")
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.add_below(id, window, cx);
            }));
        h_flex()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(self.gutter() - 4.))
            .justify_end()
            .when(!self.is_selected(ix), |h| {
                h.opacity(0.)
                    .group_hover(format!("blk-{id}"), |s| s.opacity(1.))
            })
            .child(add)
            .child(self.render_grip(ix, theme, cx))
            .into_any_element()
    }
}

/// Under a custom block's config while it's edited: which block it is and the params it takes.
fn block_hint(kind: &Kind, theme: &Theme, cx: &App) -> Option<AnyElement> {
    let Kind::Code(lang) = kind else { return None };
    let name = blocks::model::fence_type(lang)?;
    let text = match blocks::find(name, cx) {
        Some(ty) if ty.params.is_empty() => ty.label().to_string(),
        Some(ty) => {
            let params = ty
                .params
                .iter()
                .map(|p| {
                    let kind = match p.kind {
                        ParamKind::Text => "text",
                        ParamKind::Number => "number",
                        ParamKind::Bool => "yes/no",
                    };
                    if p.default.is_empty() {
                        format!("{} ({kind})", p.name)
                    } else {
                        format!("{} ({kind}, default {})", p.name, p.default)
                    }
                })
                .collect::<Vec<_>>()
                .join(" · ");
            format!("{} — {params}", ty.label())
        }
        None => format!("Unknown block type “{name}”"),
    };
    Some(
        div()
            .mt_1()
            .px_3()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text)
            .into_any_element(),
    )
}
