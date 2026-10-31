//! App asset source: Ely's bundled fonts and Lucide icons, plus the few Lucide
//! icons Desktop uses that Ely does not bundle (served under `zcode/`).

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub(crate) const BRAIN_ICON: &str = "zcode/brain.svg";
/// Full-colour GCode mark above the draft greeting.
pub(crate) const GCODE_MARK: &str = "zcode/gcode-mark.svg";
/// Its 64×64 source; the greeting rasterises it at device-pixel size.
pub(crate) const GCODE_MARK_SVG: &[u8] = include_bytes!("../../assets/brand/gcode-mark.svg");
pub(crate) const SETTINGS_2_ICON: &str = "zcode/settings-2.svg";
pub(crate) const BLOCKS_ICON: &str = "zcode/blocks.svg";
pub(crate) const CABLE_ICON: &str = "zcode/cable.svg";

const LOCAL: &[(&str, &[u8])] = &[
    (BRAIN_ICON, include_bytes!("../../assets/icons/brain.svg")),
    (GCODE_MARK, GCODE_MARK_SVG),
    (
        SETTINGS_2_ICON,
        include_bytes!("../../assets/icons/settings-2.svg"),
    ),
    (BLOCKS_ICON, include_bytes!("../../assets/icons/blocks.svg")),
    (CABLE_ICON, include_bytes!("../../assets/icons/cable.svg")),
];

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = LOCAL.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        ely_gpui_component::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut names = ely_gpui_component::Assets.list(path)?;
        names.extend(
            LOCAL
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_local_and_ely_icons() {
        let brain = AppAssets.load(BRAIN_ICON).unwrap().unwrap();
        assert!(
            std::str::from_utf8(&brain)
                .unwrap()
                .contains("lucide-brain")
        );
        assert!(AppAssets.load("icons/plus.svg").unwrap().is_some());
    }

    #[test]
    fn every_local_icon_constant_is_served() {
        for path in [
            BRAIN_ICON,
            GCODE_MARK,
            SETTINGS_2_ICON,
            BLOCKS_ICON,
            CABLE_ICON,
        ] {
            assert!(AppAssets.load(path).unwrap().is_some(), "{path}");
        }
    }
}
