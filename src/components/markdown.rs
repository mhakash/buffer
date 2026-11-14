//! Markdown rendering shared by task notes and standalone notes: text styles, Mermaid diagrams
//! (```mermaid fences rendered to SVG natively), YAML frontmatter shown as property rows, and
//! image paths relative to the note's folder.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use gpui_kit::component::{
    ActiveTheme, h_flex,
    text::{
        MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView,
        TextViewStyle, markdown_ast,
    },
    v_flex,
};
use gpui_kit::{
    App, ElementId, Image, ImageFormat, ImageSource, IntoElement, ParentElement, SharedString,
    SharedUri, Styled, Window, div, img, px, rems, rgb,
};

/// Markdown notes sit under small section labels: headings stay close to the body size
/// (hierarchy from weight) and paragraphs tight.
pub fn markdown_style() -> TextViewStyle {
    TextViewStyle::default()
        .paragraph_gap(rems(0.5))
        .heading_font_size(|level, _| match level {
            1 => px(18.),
            2 => px(16.),
            3 => px(15.),
            _ => px(14.),
        })
}

/// Full-width notes read like a document: body `text_base`, real heading steps.
pub fn document_style() -> TextViewStyle {
    TextViewStyle::default().heading_font_size(|level, _| match level {
        1 => px(26.),
        2 => px(21.),
        3 => px(18.),
        _ => px(16.),
    })
}

/// A Markdown view with the app's extras. `dir` is the note's folder: relative image paths
/// resolve against it.
pub fn markdown_view(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    style: TextViewStyle,
    dir: PathBuf,
) -> TextView {
    TextView::markdown(id, text)
        .style(style)
        .selectable(true)
        .image_source(move |uri| resolve_image(uri, &dir))
        // Replaces the extension set, so it goes before the plugins.
        .markdown_extensions(MarkdownExtensions::default().frontmatter())
        .plugin(FrontmatterPlugin)
        .plugin(MermaidPlugin)
}

/// Remote URLs load through the app's HTTP client (`net.rs`); `data:` URLs decode inline;
/// `file://`, absolute, `~/` and relative paths load from disk.
fn resolve_image(uri: &SharedUri, dir: &Path) -> ImageSource {
    let url = uri.as_ref();
    if url.starts_with("http://") || url.starts_with("https://") {
        return uri.clone().into();
    }
    if url.starts_with("data:") {
        return data_image(url).map_or_else(|| uri.clone().into(), ImageSource::Image);
    }
    let path = url.strip_prefix("file://").unwrap_or(url);
    let path = match path.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => dir.join(path), // `join` keeps absolute paths as they are
    };
    path.into()
}

fn data_image(url: &str) -> Option<Arc<Image>> {
    let data = data_url::DataUrl::process(url).ok()?;
    let mime = data.mime_type();
    let format = ImageFormat::from_mime_type(&format!("{}/{}", mime.type_, mime.subtype))?;
    let (bytes, _) = data.decode_to_vec().ok()?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

const MERMAID: &str = "mermaid";

/// Renders ```mermaid code fences as diagrams; invalid diagrams show the error and source.
struct MermaidPlugin;

struct Diagram {
    source: String,
}

/// Rendered SVG (or the parse error) per (source, dark), so each diagram renders once.
type Rendered = Result<(Arc<Image>, f32, f32), String>;
static CACHE: LazyLock<Mutex<HashMap<(u64, bool), Rendered>>> = LazyLock::new(Default::default);

fn render_diagram(source: &str, dark: bool) -> Rendered {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), dark);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| {
            let options = mermaid_rs_renderer::RenderOptions {
                theme: if dark {
                    mermaid_rs_renderer::Theme::dark()
                } else {
                    mermaid_rs_renderer::Theme::modern()
                },
                ..Default::default()
            };
            let size = mermaid_rs_renderer::measure(source, options.clone())
                .map_err(|e| format!("{e:#}"))?;
            let svg = mermaid_rs_renderer::render_with_options(source, options)
                .map_err(|e| format!("{e:#}"))?;
            Ok((
                Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
                size.width,
                size.height,
            ))
        })
        .clone()
}

impl MarkdownPlugin for MermaidPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        MERMAID
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else {
            return None;
        };
        if !code
            .lang
            .as_deref()
            .is_some_and(|l| l.eq_ignore_ascii_case(MERMAID))
        {
            return None;
        }
        Some(
            MarkdownNode::new(
                MERMAID,
                Diagram {
                    source: code.value.clone(),
                },
            )
            .text(code.value.clone())
            .markdown(cx.node_source(node).unwrap_or(&code.value).to_string()),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let source = node
            .data::<Diagram>()
            .map(|d| d.source.as_str())
            .unwrap_or_default();
        let dark = theme.is_dark();
        // The renderer paints its own canvas (white / #333); match it so the diagram doesn't
        // sit as a card inside a card.
        let frame = v_flex()
            .w_full()
            .my_2()
            .p_4()
            .gap_2()
            .rounded_xl()
            .border_1()
            .border_color(theme.border)
            .bg(rgb(if dark { 0x333333 } else { 0xffffff }));
        match render_diagram(source, dark) {
            // Natural size, shrunk to the column. `img` alone keeps its intrinsic height when
            // shrunk, so a box sized by aspect ratio carries the layout and the image fills it.
            Ok((image, w, h)) => frame.items_center().child(
                div()
                    .w_full()
                    .max_w(px(w))
                    .aspect_ratio(w / h.max(1.))
                    .child(img(image).size_full()),
            ),
            Err(err) => frame
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(format!("Mermaid: {err}")),
                )
                .child(
                    div()
                        .text_sm()
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground)
                        .whitespace_normal()
                        .child(source.to_string()),
                ),
        }
    }
}

const FRONTMATTER: &str = "frontmatter";

/// A leading `---` YAML block, shown like the note's own properties.
struct FrontmatterPlugin;

/// Top-level `key: value` pairs; lists (`[a, b]` or `- a` lines) become several values.
#[derive(Debug, PartialEq)]
struct Frontmatter(Vec<(String, Vec<String>)>);

fn parse_frontmatter(yaml: &str) -> Frontmatter {
    let unquote = |v: &str| v.trim().trim_matches(['"', '\'']).to_string();
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    for line in yaml.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        if let (false, Some((key, value))) = (indented, line.split_once(':')) {
            let value = value.trim();
            let values = match value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                Some(list) => list
                    .split(',')
                    .map(unquote)
                    .filter(|v| !v.is_empty())
                    .collect(),
                None if value.is_empty() => Vec::new(),
                None => vec![unquote(value)],
            };
            entries.push((key.trim().to_string(), values));
        } else if let Some((_, values)) = entries.last_mut() {
            values.push(unquote(line.trim().trim_start_matches("- ")));
        }
    }
    Frontmatter(entries)
}

impl MarkdownPlugin for FrontmatterPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        FRONTMATTER
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Yaml(yaml) = node else {
            return None;
        };
        Some(
            MarkdownNode::new(FRONTMATTER, parse_frontmatter(&yaml.value))
                .text(yaml.value.clone())
                .markdown(cx.node_source(node).unwrap_or(&yaml.value).to_string()),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let entries = node
            .data::<Frontmatter>()
            .map(|f| f.0.as_slice())
            .unwrap_or_default();
        v_flex()
            .w_full()
            .mb_2()
            .pb_2()
            .border_b_1()
            .border_color(theme.border)
            .children(entries.iter().map(|(key, values)| {
                let value = if values.len() > 1 {
                    // Lists read as tags.
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(values.iter().map(|v| {
                            div()
                                .px_2()
                                .rounded_full()
                                .bg(theme.secondary)
                                .text_xs()
                                .child(v.clone())
                        }))
                        .into_any_element()
                } else {
                    div()
                        .min_w_0()
                        .text_sm()
                        .child(values.first().cloned().unwrap_or_default())
                        .into_any_element()
                };
                super::property_row(gpui_kit::assets::IconName::FileText, key.clone(), theme)
                    .child(value)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter_scalars_and_lists() {
        let f = parse_frontmatter(
            "title: \"What\"\ndate: 2026-03-26\ntags: [\"a\", b]\nowners:\n  - Ann\n  - Bo\n# note\nempty:",
        );
        assert_eq!(
            f.0,
            vec![
                ("title".into(), vec!["What".into()]),
                ("date".into(), vec!["2026-03-26".into()]),
                ("tags".into(), vec!["a".into(), "b".into()]),
                ("owners".into(), vec!["Ann".into(), "Bo".into()]),
                ("empty".into(), vec![]),
            ]
        );
    }
}
