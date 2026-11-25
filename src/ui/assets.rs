//! Embedded assets (Heroicons, MIT license in assets/icons/LICENSE).

use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/cog-6-tooth.svg" => include_bytes!("../../assets/icons/cog-6-tooth.svg"),
            "icons/minus.svg" => include_bytes!("../../assets/icons/minus.svg"),
            "icons/square.svg" => include_bytes!("../../assets/icons/square.svg"),
            "icons/square-2-stack.svg" => include_bytes!("../../assets/icons/square-2-stack.svg"),
            "icons/x-mark.svg" => include_bytes!("../../assets/icons/x-mark.svg"),
            "icons/plus.svg" => include_bytes!("../../assets/icons/plus.svg"),
            "icons/magnifying-glass.svg" => {
                include_bytes!("../../assets/icons/magnifying-glass.svg")
            }
            "icons/chevron-down.svg" => include_bytes!("../../assets/icons/chevron-down.svg"),
            "icons/chevron-up.svg" => include_bytes!("../../assets/icons/chevron-up.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
