//! Thin wrappers over gpui-kit components that pin the app's look, plus visual effects.

mod assets;
pub mod aurora;
mod block_controls;
mod card;
mod chip;
pub mod confirm;
mod drag_scroll;
pub mod file_editor;
mod file_list;
pub mod fx;
pub mod lightbox;
mod markdown;
mod outline;
mod property;

pub use assets::Assets;
pub use block_controls::{BlockHost, preview_controls};
pub use card::card;
pub use chip::{category_tag, chip, icon_button};
pub use drag_scroll::DragScroll;
pub use file_list::file_list;
pub use markdown::{
    document_style, hosted_markdown_view, markdown_style, markdown_view, open_link, output_view,
    take_claimed_click,
};
pub use outline::Outline;
pub use property::property_row;
