//! Thin wrappers over gpui-kit components that pin the app's look, plus visual effects.

mod assets;
pub mod aurora;
mod card;
mod chip;
pub mod confirm;
pub mod file_editor;
mod file_list;
pub mod fx;
pub mod lightbox;
mod markdown;
mod property;

pub use assets::Assets;
pub use card::card;
pub use chip::{category_tag, chip, icon_button};
pub use file_list::file_list;
pub use markdown::{
    document_style, markdown_style, markdown_view, output_view, take_claimed_click,
};
pub use property::property_row;
