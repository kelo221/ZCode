//! Workspace shell geometry from Desktop (`app-shell/WorkspaceShellLayout.tsx`,
//! `v4/conversationLayout.ts`, `workspaceShellWindowChrome.ts`).

/// Sidebar width (Desktop minimum; its resizable maximum is half the window).
pub(crate) const SIDEBAR_WIDTH: f32 = 264.;
/// Panel corner radius on Windows.
pub(crate) const PANEL_RADIUS: f32 = 5.;
/// Workspace header height (`h-12`).
pub(crate) const HEADER_HEIGHT: f32 = 48.;
/// Docked side pane width.
pub(crate) const SIDE_PANE_WIDTH: f32 = 400.;
/// The centre column never shrinks below this (`min-w-[320px]`).
pub(crate) const CENTER_MIN_WIDTH: f32 = 320.;
/// Gap between the docked panels and the window edge.
pub(crate) const PANEL_GAP: f32 = 4.;

/// Width of the conversation column inside a centre panel `container` wide.
/// Draft (empty) views use the narrower `max-w-2xl`.
pub(crate) fn conversation_width(container: f32, draft: bool) -> f32 {
    let padded = (container - 2. * conversation_padding(container)).max(0.);
    if draft {
        return padded.min(672.);
    }
    if container >= 1280. {
        (container - 384.).min(1152.)
    } else if container >= 864. {
        (container - 96.).min(896.)
    } else {
        padded
    }
}

/// Horizontal padding of the conversation area (`px-4 md:px-6`).
pub(crate) fn conversation_padding(container: f32) -> f32 {
    if container >= 768. { 24. } else { 16. }
}

/// Whether the side pane fits beside a usable centre column; Desktop collapses
/// it automatically when it would not.
pub(crate) fn side_pane_fits(window_width: f32, sidebar_visible: bool) -> bool {
    let sidebar = if sidebar_visible { SIDEBAR_WIDTH } else { 0. };
    window_width - sidebar - SIDE_PANE_WIDTH - 3. * PANEL_GAP >= CENTER_MIN_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_width_follows_desktop_breakpoints() {
        assert_eq!(conversation_width(700., false), 668.);
        assert_eq!(conversation_width(900., false), 804.);
        assert_eq!(conversation_width(1100., false), 896.);
        assert_eq!(conversation_width(1400., false), 1016.);
        assert_eq!(conversation_width(1800., false), 1152.);
        assert_eq!(conversation_width(1800., true), 672.);
        assert_eq!(conversation_width(500., true), 468.);
    }

    #[test]
    fn side_pane_collapses_when_centre_would_be_too_narrow() {
        assert!(side_pane_fits(1280., true));
        assert!(!side_pane_fits(960., true));
        assert!(side_pane_fits(960., false));
    }
}
