//! "Files in this note": every local file a note's Markdown links to (uploaded, linked or made
//! in the app), read back from the content — no separate index. Shown under the note body and
//! under task notes. Click opens one (text files in the editor dialog); hover shows it in Finder.

use std::path::Path;

use gpui_kit::component::{Icon, Sizable, Theme, h_flex, v_flex};
use gpui_kit::{AnyElement, ElementId, SharedString, assets::IconName, div, prelude::*, px};

use super::{file_editor, icon_button};
use crate::files;

/// The list for `markdown` (relative links resolve against `dir`), or `None` without file
/// links. `id` prefixes element ids (unique per view).
pub fn file_list(
    id: &'static str,
    markdown: &str,
    dir: &Path,
    theme: &Theme,
) -> Option<AnyElement> {
    let links = files::local_links(markdown);
    if links.is_empty() {
        return None;
    }
    let rows = links.into_iter().enumerate().map(|(ix, link)| {
        let path = files::resolve(&link.target, dir);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| link.target.clone());
        let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file());
        let missing = meta.is_none();
        let detail = match (&meta, path.starts_with(dir)) {
            (None, _) => "Missing".to_string(),
            (Some(m), true) => files::human_size(m.len()),
            (Some(m), false) => format!("Linked · {}", files::human_size(m.len())),
        };
        let icon = match files::kind(&name) {
            files::Kind::Text => IconName::FileText,
            files::Kind::Image => IconName::FileImage,
            files::Kind::Other => IconName::File,
        };
        let group = SharedString::from(format!("{id}-{ix}"));
        let (open, reveal) = (path.clone(), path);
        h_flex()
            .id(ElementId::NamedInteger(id.into(), ix as u64))
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
                                ElementId::NamedInteger(format!("{id}-reveal").into(), ix as u64),
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
