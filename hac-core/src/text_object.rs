pub mod cursor;
#[allow(clippy::module_inception)]
mod text_object;
mod character;
mod line_break;

pub use text_object::{Readonly, TextObject, Write};
