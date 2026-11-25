//! The block being edited, Notion-style: formatting shows as you type and Markdown markers
//! stay hidden (the block's text is still Markdown underneath).
//!
//! | File | Concern |
//! |---|---|
//! | `inline` | Markdown ⇄ shown text: hidden markers, styles, offset mapping, balanced split (pure, tested) |
//! | `state` | `RichTextState`: text, selection, editing actions, undo, platform text input |
//! | `element` | `RichText`: draws it (text, selection, caret) and takes the mouse |

mod element;
pub mod inline;
mod state;

pub use element::{RichText, rich_text_view};
pub use state::RichTextState;
