//! Widgets shared by the root view: top bar, tabs and window controls,
//! built on the primitives in [`ui`].

mod tabs;
mod top_bar;
mod ui;
mod window_controls;

pub(crate) use ui::{Combobox, bind_text_input_keys, content_card, icon};
pub(crate) use window_controls::window_controls;
