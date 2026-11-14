use gpui_kit::{component::text::TextViewStyle, px, rems};

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
