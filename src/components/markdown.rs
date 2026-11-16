//! Markdown rendering shared by task notes and standalone notes: text styles, Mermaid diagrams
//! (```mermaid fences rendered to SVG natively), YAML frontmatter shown as property rows, and
//! image paths relative to the note's folder.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use gpui_kit::assets::IconName;
use gpui_kit::component::Theme;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonRounded},
    h_flex,
    text::{
        MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView,
        TextViewStyle, markdown_ast,
    },
    v_flex,
};
use gpui_kit::{
    App, CursorStyle, ElementId, FontWeight, Hsla, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Rgba, SharedString, SharedUri,
    StatefulInteractiveElement, Styled, StyledImage, Window, div, img, prelude::FluentBuilder as _,
    px, rems,
};

use super::lightbox;
use crate::blocks::{self, model::Config};

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
    output_view(id, text, style, dir.clone()).plugin(BlockPlugin { dir })
}

/// Everything but programmatic blocks: a block's own output can't run more blocks.
pub fn output_view(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    style: TextViewStyle,
    dir: PathBuf,
) -> TextView {
    let images = dir.clone();
    TextView::markdown(id, text)
        .style(style)
        .selectable(true)
        .image_source(move |uri| resolve_image(uri, &images))
        // Replaces the extension set, so it goes before the plugins.
        .markdown_extensions(MarkdownExtensions::default().frontmatter())
        .plugin(FrontmatterPlugin)
        .plugin(MermaidPlugin)
        .plugin(ImagePlugin { dir })
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

/// A rendered diagram: SVG text (re-scaled for the full view), shown image, natural size.
#[derive(Clone)]
struct Rendered {
    svg: Arc<str>,
    image: Arc<Image>,
    size: (f32, f32),
}

/// Rendered diagram (or the parse error) per (source, palette), so each renders once.
static CACHE: LazyLock<Mutex<HashMap<u64, Result<Rendered, String>>>> =
    LazyLock::new(Default::default);

/// Colors as `#rrggbb` for the SVG.
fn hex(color: Hsla) -> String {
    let c = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

/// `base` tinted `amount` (0–1) toward `tint`, opaque.
fn mix(base: Hsla, tint: Hsla, amount: f32) -> Hsla {
    let (a, b) = (Rgba::from(base.opacity(1.)), Rgba::from(tint.opacity(1.)));
    let lerp = |x: f32, y: f32| x + (y - x) * amount;
    Rgba {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: 1.,
    }
    .into()
}

/// The diagram card's surface: opaque so node and label fills can match it exactly.
fn diagram_surface(theme: &Theme) -> Hsla {
    theme.popover.opacity(1.)
}

/// Mermaid colors from the app palette: accent-tinted nodes on the card's surface, muted
/// lines, pill labels. Transparent canvas so the card shows through.
fn diagram_theme(theme: &Theme) -> mermaid_rs_renderer::Theme {
    let dark = theme.is_dark();
    let surface = diagram_surface(theme);
    let accent = theme.primary;
    let node = hex(mix(surface, accent, if dark { 0.16 } else { 0.07 }));
    let border = hex(mix(surface, accent, if dark { 0.5 } else { 0.32 }));
    let line = hex(mix(surface, theme.muted_foreground, 0.8));
    let text = hex(theme.foreground);
    let note = hex(mix(surface, theme.chart_4, if dark { 0.18 } else { 0.12 }));
    let note_border = hex(mix(surface, theme.chart_4, 0.45));
    let charts = [
        theme.chart_1,
        theme.chart_2,
        theme.chart_3,
        theme.chart_4,
        theme.chart_5,
    ];
    let mut t = mermaid_rs_renderer::Theme::modern();
    t.font_family = "\"SF Pro Text\", \"Helvetica Neue\", Helvetica, Arial, sans-serif".into();
    t.font_size = 14.;
    t.background = "none".into();
    t.primary_color = node.clone();
    t.secondary_color = node.clone();
    t.tertiary_color = node.clone();
    t.primary_border_color = border.clone();
    t.primary_text_color = text.clone();
    t.text_color = text.clone();
    t.line_color = line.clone();
    t.edge_label_background = hex(surface);
    t.cluster_background = hex(mix(surface, accent, if dark { 0.06 } else { 0.03 }));
    t.cluster_border = border.clone();
    t.sequence_actor_fill = node.clone();
    t.sequence_actor_border = border.clone();
    t.sequence_actor_line = line;
    t.sequence_note_fill = note;
    t.sequence_note_border = note_border;
    t.sequence_activation_fill = node;
    t.sequence_activation_border = border.clone();
    t.pie_colors = std::array::from_fn(|i| {
        let amount = if i < 5 { 0.85 } else { 0.5 };
        hex(mix(surface, charts[i % 5], amount))
    });
    t.pie_title_text_color = text.clone();
    t.pie_section_text_color = text.clone();
    t.pie_legend_text_color = text;
    t.pie_stroke_color = hex(surface);
    t.pie_outer_stroke_color = border;
    t.pie_opacity = 1.;
    t
}

/// Mermaid.js strips quotes from `|"label"|` edge labels; the renderer shows them literally.
fn unquote_edge_labels(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("|\"") {
        let after = &rest[start + 2..];
        // Only within one line, so a stray `|"` can't swallow the rest of the diagram.
        let line_end = after.find('\n').unwrap_or(after.len());
        match after[..line_end].find("\"|") {
            Some(end) => {
                out.push_str(&rest[..start + 1]);
                out.push_str(&after[..end]);
                out.push('|');
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[..start + 2]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Softer shapes than the renderer draws: rounder boxes, pill edge labels, thinner edges.
fn polish_svg(svg: &str) -> String {
    let svg = svg
        .replace("rx=\"3\" ry=\"3\"", "rx=\"10\" ry=\"10\"")
        // SVG clamps the radius to half the box: pills.
        .replace("rx=\"2\" ry=\"2\"", "rx=\"99\" ry=\"99\"");
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg.as_str();
    while let Some(ix) = rest.find("class=\"edgePath\"") {
        let (head, tail) = rest.split_at(ix);
        out.push_str(head);
        let tag_end = tail.find("/>").unwrap_or(tail.len());
        out.push_str(&tail[..tag_end].replacen("stroke-width=\"2\"", "stroke-width=\"1.5\"", 1));
        rest = &tail[tag_end..];
    }
    out.push_str(rest);
    out
}

/// The same SVG drawn `scale`× larger, so the full view rasterizes sharp.
fn scale_svg(svg: &str, (w, h): (f32, f32), scale: f32) -> String {
    let Some(tag_end) = svg.find('>') else {
        return svg.to_string();
    };
    let (root, body) = svg.split_at(tag_end);
    let mut root = root.to_string();
    for (attr, value) in [("width", w), ("height", h)] {
        let key = format!(" {attr}=\"");
        if let Some(start) = root.find(&key) {
            let value_start = start + key.len();
            if let Some(len) = root[value_start..].find('"') {
                root.replace_range(
                    value_start..value_start + len,
                    &format!("{}", value * scale),
                );
            }
        }
    }
    root + body
}

fn render_diagram(source: &str, theme: &Theme) -> Result<Rendered, String> {
    let palette = diagram_theme(theme);
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    serde_json::to_string(&palette)
        .unwrap_or_default()
        .hash(&mut hasher);
    let key = hasher.finish();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| {
            let mut options = mermaid_rs_renderer::RenderOptions {
                theme: palette,
                ..Default::default()
            };
            // Airier than the defaults: fewer crossings and labels that clear the lines.
            options.layout.node_spacing = 60.;
            options.layout.rank_spacing = 64.;
            options.layout.node_padding_x = 22.;
            options.layout.node_padding_y = 14.;
            let source = unquote_edge_labels(source);
            let size = mermaid_rs_renderer::measure(&source, options.clone())
                .map_err(|e| format!("{e:#}"))?;
            let svg = mermaid_rs_renderer::render_with_options(&source, options)
                .map_err(|e| format!("{e:#}"))?;
            let svg = polish_svg(&svg);
            Ok(Rendered {
                image: Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    svg.clone().into_bytes(),
                )),
                svg: svg.into(),
                size: (size.width, size.height),
            })
        })
        .clone()
}

/// Opens a diagram in the full view, re-rendered large enough to stay sharp.
fn open_diagram(diagram: &Rendered, window: &mut Window, cx: &mut App) {
    let view = window.viewport_size();
    let (w, h) = diagram.size;
    let fit = (f32::from(view.width) / w)
        .min(f32::from(view.height) / h)
        .clamp(1., 2.);
    let image = if fit > 1.05 {
        let svg = scale_svg(&diagram.svg, diagram.size, fit);
        Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()))
    } else {
        diagram.image.clone()
    };
    let canvas = lightbox::Canvas {
        size: diagram.size,
        surface: diagram_surface(cx.theme()),
    };
    lightbox::open(ImageSource::Image(image), Some(canvas), window, cx);
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
        let frame = v_flex()
            .w_full()
            .my_2()
            .p_5()
            .gap_2()
            .rounded_2xl()
            .border_1()
            .border_color(theme.border)
            .bg(diagram_surface(theme));
        match render_diagram(source, theme) {
            // Natural size, shrunk to the column. `img` alone keeps its intrinsic height when
            // shrunk, so a box sized by aspect ratio carries the layout and the image fills it.
            Ok(diagram) => {
                let (w, h) = diagram.size;
                let mut hasher = DefaultHasher::new();
                source.hash(&mut hasher);
                frame.items_center().child(
                    div()
                        .id(("mermaid", hasher.finish()))
                        .w_full()
                        .max_w(px(w))
                        .aspect_ratio(w / h.max(1.))
                        .cursor(CursorStyle::PointingHand)
                        .child(img(diagram.image.clone()).size_full())
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            open_diagram(&diagram, window, cx);
                        }),
                )
            }
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

const IMAGE: &str = "image";

/// Paragraphs holding only images render them as blocks that open the full view on click.
/// Images inside running text stay with the kit's inline renderer.
struct ImagePlugin {
    dir: PathBuf,
}

struct Images(Vec<(SharedUri, SharedString)>);

impl MarkdownPlugin for ImagePlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        IMAGE
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Paragraph(paragraph) = node else {
            return None;
        };
        let mut images = Vec::new();
        for child in &paragraph.children {
            match child {
                markdown_ast::Node::Image(image) => images.push((
                    SharedUri::from(image.url.clone()),
                    SharedString::from(image.alt.clone()),
                )),
                markdown_ast::Node::Text(text) if text.value.trim().is_empty() => {}
                markdown_ast::Node::Break(_) => {}
                _ => return None,
            }
        }
        if images.is_empty() {
            return None;
        }
        let alt = images
            .iter()
            .map(|(_, alt)| alt.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let source = cx.node_source(node).unwrap_or_default().to_string();
        Some(
            MarkdownNode::new(IMAGE, Images(images))
                .text(alt)
                .markdown(source),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        let images = node
            .data::<Images>()
            .map(|i| i.0.as_slice())
            .unwrap_or_default();
        v_flex()
            .w_full()
            .gap_2()
            .children(images.iter().map(|(uri, _)| {
                let source = resolve_image(uri, &self.dir);
                let mut hasher = DefaultHasher::new();
                uri.hash(&mut hasher);
                img(source.clone())
                    .id(("md-image", hasher.finish()))
                    .object_fit(ObjectFit::Contain)
                    .max_w_full()
                    .rounded_lg()
                    .cursor(CursorStyle::PointingHand)
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        lightbox::open(source.clone(), None, window, cx);
                    })
            }))
    }
}

const BLOCK: &str = "block";

/// ```` ```block <name> ```` fences: runs the block type's script (see `crate::blocks`) and shows
/// its Markdown in a card. Results are cached; stale ones re-run when shown.
struct BlockPlugin {
    dir: PathBuf,
}

struct Fence {
    name: String,
    config: Config,
    source: String,
}

impl MarkdownPlugin for BlockPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        BLOCK
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else {
            return None;
        };
        let info = format!(
            "{} {}",
            code.lang.as_deref().unwrap_or_default(),
            code.meta.as_deref().unwrap_or_default()
        );
        let name = blocks::model::fence_type(&info)?.to_string();
        Some(
            MarkdownNode::new(
                BLOCK,
                Fence {
                    name,
                    config: blocks::model::parse_config(&code.value),
                    source: code.value.clone(),
                },
            )
            .text(code.value.clone())
            .markdown(cx.node_source(node).unwrap_or(&code.value).to_string()),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let card = v_flex()
            .w_full()
            .my_2()
            .p_4()
            .gap_2()
            .rounded_2xl()
            .border_1()
            .border_color(theme.border)
            .bg(theme.group_box);
        let Some(fence) = node.data::<Fence>() else {
            return card.into_any_element();
        };
        let Some(ty) = blocks::find(&fence.name, cx).cloned() else {
            return card
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(format!("Unknown block type “{}”", fence.name)),
                )
                .child(
                    div()
                        .text_sm()
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground)
                        .whitespace_normal()
                        .child(fence.source.clone()),
                )
                .into_any_element();
        };
        blocks::ensure(&ty, &fence.config, cx);
        let status = blocks::status(&ty, &fence.config, cx);
        let key = ty.key(&fence.config);
        let id = u64::from_str_radix(&key, 16).unwrap_or_default();
        let group: SharedString = format!("block-{key}").into();
        let now = chrono::Utc::now();

        let trusted = crate::workspaces::blocks_trusted(cx);
        let note = match (&status.output, status.running) {
            _ if !trusted => "Scripts off".to_string(),
            (_, true) => "Running…".to_string(),
            (Some(out), _) if out.ok => format!("Updated {}", blocks::model::ago(out.ran_at, now)),
            (Some(out), _) => format!("Failed {}", blocks::model::ago(out.ran_at, now)),
            (None, false) => "Not run yet".to_string(),
        };
        let icon = Icon::empty()
            .path(blocks::icon_path(&ty.icon, cx))
            .small()
            .text_color(theme.muted_foreground);
        let ambient = crate::prefs::get(cx).ambient;
        let refresh = {
            let (ty, config) = (ty.clone(), fence.config.clone());
            super::icon_button(
                ("block-refresh", id),
                IconName::RefreshCw,
                theme.muted_foreground,
                "Refresh",
            )
            .xsmall()
            .disabled(status.running)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                blocks::refresh(&ty, &config, cx);
            })
        };
        let header = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(if status.running {
                super::fx::pulse(icon, ("block-pulse", id), ambient, 0.35)
            } else {
                icon.into_any_element()
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(ty.label().to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(note),
            )
            .when(trusted, |h| {
                h.child(
                    div()
                        .opacity(0.)
                        .group_hover(group.clone(), |s| s.opacity(1.))
                        .child(refresh),
                )
            });
        let allow = (!trusted).then(|| {
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Block scripts are off in this workspace."),
                )
                .child(
                    Button::new(("block-allow", id))
                        .small()
                        .rounded(ButtonRounded::Size(px(999.)))
                        .label("Allow…")
                        .on_click(|_, window, cx| {
                            cx.stop_propagation();
                            allow_blocks(window, cx);
                        }),
                )
        });

        let body = status.output.as_ref().and_then(|out| {
            (!out.markdown.trim().is_empty()).then(|| {
                output_view(
                    ("block-out", id),
                    out.markdown.clone(),
                    markdown_style(),
                    self.dir.clone(),
                )
                .into_any_element()
            })
        });
        let error = status.output.as_ref().filter(|o| !o.ok).map(|out| {
            let open = blocks::details_open(&key, cx);
            let first = out.stderr.lines().next().unwrap_or_default().to_string();
            let more = out.stderr.lines().nth(1).is_some();
            let toggle_key = key.clone();
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .id(("block-err", id))
                        .gap_1()
                        .items_center()
                        .text_xs()
                        .text_color(theme.danger)
                        .when(more, |d| {
                            d.cursor(CursorStyle::PointingHand)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    blocks::toggle_details(&toggle_key, cx);
                                })
                        })
                        .when(more, |d| {
                            d.child(
                                Icon::new(if open {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .xsmall()
                                .text_color(theme.danger),
                            )
                        })
                        .child(div().min_w_0().truncate().child(first)),
                )
                .when(open, |d| {
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

        card.group(group)
            .child(header)
            .children(allow)
            .children(body)
            .children(error)
            .into_any_element()
    }
}

/// Asks before letting this workspace's custom blocks run their scripts.
fn allow_blocks(window: &mut Window, cx: &mut App) {
    let name = crate::workspaces::get(cx).active().name.clone();
    super::confirm::confirm(
        window,
        cx,
        "Allow block scripts?",
        format!(
            "Custom blocks in “{name}” run the scripts in its blocks folder on this Mac, with \
             your access to files and the network. Allow only if you trust what's in that folder. \
             You can turn this off in Settings."
        ),
        "Allow",
        |_, cx| crate::workspaces::set_blocks_trusted(true, cx),
    );
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
                super::property_row(IconName::FileText, key.clone(), theme).child(value)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquotes_edge_labels() {
        assert_eq!(
            unquote_edge_labels("A -->|\"read\"| B\nC -->|plain| D\nE --> F[\"|\"]"),
            "A -->|read| B\nC -->|plain| D\nE --> F[\"|\"]"
        );
    }

    #[test]
    fn scales_svg_root_size() {
        assert_eq!(
            scale_svg(
                "<svg width=\"10\" height=\"20\" viewBox=\"0 0 10 20\"><rect width=\"10\"/>",
                (10., 20.),
                2.
            ),
            "<svg width=\"20\" height=\"40\" viewBox=\"0 0 10 20\"><rect width=\"10\"/>"
        );
    }

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
