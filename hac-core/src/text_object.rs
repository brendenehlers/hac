mod character;
pub mod cursor;
mod line_break;
#[allow(clippy::module_inception)]
mod text_object;

pub use text_object::{Readonly, TextObject, Write};
