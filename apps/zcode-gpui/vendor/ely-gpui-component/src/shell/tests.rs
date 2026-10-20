use gpui::{
    AppContext as _, Context, FocusHandle, InteractiveElement, IntoElement, KeyUpEvent, Keystroke,
    ParentElement, Render, TestAppContext, VisualTestContext, Window, div, prelude::*, px, size,
};

use super::{WindowManager, WindowSwitcher};
use crate::{buttons::Button, primitives::FocusScope, theme::Theme};

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Desk {
    root: FocusHandle,
    open: bool,
    enters_above: usize,
}

impl Render for Desk {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (opener, closer, above) = (cx.entity(), cx.entity(), cx.entity());
        let scope = FocusScope::new(&self.root)
            .size_full()
            .child(Button::new("opener", "Windows").on_click(move |_, _, cx| {
                opener.update(cx, |desk, cx| {
                    desk.open = true;
                    cx.notify();
                })
            }))
            .when(self.open, |desk| {
                desk.child(WindowSwitcher::new("switcher", move |_, cx| {
                    closer.update(cx, |desk, cx| {
                        desk.open = false;
                        cx.notify();
                    })
                }))
            });
        div()
            .size_full()
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "enter" {
                    above.update(cx, |desk, _| desk.enters_above += 1);
                }
            })
            .child(scope)
    }
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn press(key: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(key);
    settle(cx);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).expect("a key"),
    });
    settle(cx);
}

#[gpui::test]
fn the_switcher_switches_on_release_so_its_opener_stays_shut(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (desk, cx) = cx.add_window_view(|_, cx| Desk {
        root: cx.focus_handle(),
        open: false,
        enters_above: 0,
    });
    cx.update(|_, cx| {
        WindowManager::open("Notes", size(px(320.0), px(240.0)), cx, |_, cx| {
            cx.new(|_| Blank)
        })
        .expect("a managed window");
    });
    settle(cx);
    cx.update(|window, cx| window.focus_next(cx));
    press("enter", cx);
    assert!(desk.read_with(cx, |desk, _| desk.open));
    let above = desk.read_with(cx, |desk, _| desk.enters_above);
    press("enter", cx);
    assert!(!desk.read_with(cx, |desk, _| desk.open));
    assert_eq!(desk.read_with(cx, |desk, _| desk.enters_above), above);
}

struct Platformed;

impl Render for Platformed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .child(
                div()
                    .debug_selector(|| "controls".into())
                    .child(super::WindowControls::new("controls")),
            )
            .child(
                div()
                    .debug_selector(|| "caps".into())
                    .child(crate::typography::Kbd::new("secondary-s")),
            )
    }
}

/// Window buttons and key caps follow the theme's platform: Windows draws three wide caption buttons and spells Ctrl.
#[gpui::test]
fn the_themes_platform_draws_the_buttons_and_the_caps(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    cx.update(|cx| Theme::update(cx, |theme| theme.platform = crate::theme::Platform::Mac));
    let (_, cx) = cx.add_window_view(|_, _| Platformed);
    settle(cx);
    let width = |name: &'static str, cx: &mut VisualTestContext| {
        cx.debug_bounds(name).expect("a measured part").size.width
    };
    let (lights, command) = (width("controls", cx), width("caps", cx));
    cx.update(|_, cx| Theme::update(cx, |theme| theme.platform = crate::theme::Platform::Windows));
    settle(cx);
    assert!(
        width("controls", cx) > lights * 2.0,
        "caption buttons wider than lights"
    );
    assert!(
        width("caps", cx) > command,
        "Ctrl wider than the command mark"
    );
}

struct LongTitle;

impl Render for LongTitle {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(280.0)).flex().flex_col().child(
            super::TitleBar::new("bar")
                .title("Ely — Projects at a narrow width, and more besides")
                .platform(crate::theme::Platform::Windows),
        )
    }
}

/// At 280px a Windows bar's long title gives way, and its close button ends inside the bar.
#[gpui::test]
fn a_long_title_leaves_the_caption_buttons_in_the_bar(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| LongTitle);
    settle(cx);
    let close = cx.debug_bounds("caption-Close").expect("a close button");
    assert!(close.right() <= px(280.0), "{close:?} inside the bar");
}

struct Wrapped;

impl Render for Wrapped {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let windows = crate::theme::Platform::Windows;
        div()
            .w(px(280.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "row".into())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(super::WindowControls::new("controls").platform(windows))
                    .child(div().debug_selector(|| "combo".into()).child(
                        crate::typography::KbdCombo::new("secondary-shift-k secondary-shift-s"),
                    )),
            )
            .child(div().h(px(20.0)))
    }
}

/// Caption buttons keep their own height beside caps that wrap below them, so the caps end inside the row.
#[gpui::test]
fn caption_buttons_keep_their_height_in_a_wrapping_row(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    cx.update(|cx| Theme::update(cx, |theme| theme.platform = crate::theme::Platform::Windows));
    let (_, cx) = cx.add_window_view(|_, _| Wrapped);
    settle(cx);
    let row = cx.debug_bounds("row").expect("the row");
    let combo = cx.debug_bounds("combo").expect("the caps");
    assert!(combo.top() > row.top(), "the caps wrap below the buttons");
    assert!(combo.bottom() <= row.bottom(), "{combo:?} inside {row:?}");
}

struct Bar(Option<&'static str>, crate::theme::Platform);

impl Render for Bar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let bar = super::TitleBar::new("bar").platform(self.1);
        div()
            .w(px(280.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "bar-box".into())
                    .child(match self.0 {
                        Some(title) => bar.title(title),
                        None => bar,
                    }),
            )
    }
}

/// A Windows bar with no title still keeps its buttons at its right end.
#[gpui::test]
fn an_untitled_windows_bar_keeps_its_buttons_at_the_end(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Bar(None, crate::theme::Platform::Windows));
    settle(cx);
    let close = cx.debug_bounds("caption-Close").expect("a close button");
    assert_eq!(close.right(), px(280.0));
}

/// A long centered title ends before the Linux round buttons.
#[gpui::test]
fn a_long_centered_title_ends_before_the_buttons(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let long = "Ely — Projects at a narrow width, and more besides";
    let (_, cx) = cx.add_window_view(|_, _| Bar(Some(long), crate::theme::Platform::Linux));
    settle(cx);
    let title = cx.debug_bounds("titlebar-title").expect("the title");
    let controls = cx.debug_bounds("titlebar-controls").expect("the buttons");
    assert!(
        title.right() <= controls.left(),
        "{title:?} before {controls:?}"
    );
}

/// At half the rem size the caption buttons shrink with the bar and stay inside it.
#[gpui::test]
fn caption_buttons_follow_the_windows_rem(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Bar(None, crate::theme::Platform::Windows));
    cx.update(|window, _| window.set_rem_size(px(8.0)));
    settle(cx);
    let bar = cx.debug_bounds("bar-box").expect("the bar");
    let close = cx.debug_bounds("caption-Close").expect("a close button");
    assert!(
        close.top() >= bar.top() && close.bottom() <= bar.bottom(),
        "{close:?} inside {bar:?}"
    );
}

/// A short title sits at the bar's center, whatever stands at either end.
#[gpui::test]
fn a_short_title_sits_at_the_center(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Bar(Some("Notes"), crate::theme::Platform::Linux));
    settle(cx);
    let title = cx.debug_bounds("titlebar-title").expect("the title");
    let middle = title.left() + title.size.width / 2.0;
    assert!((middle - px(140.0)).abs() < px(1.0), "{title:?} centered");
}

struct Splash;

impl Render for Splash {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(super::SplashScreen::new("Ely").progress(Some(0.5)))
    }
}

/// The splash's bar spans its column, though the column centers what it holds.
#[gpui::test]
fn the_splash_bar_spans_its_column(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Splash);
    settle(cx);
    let fill = cx.debug_bounds("progress-fill-0").expect("the bar fills");
    let width = cx.update(|window, _| window.viewport_size().width);
    let column = (width - px(80.0)) * 0.6;
    assert!(
        (fill.size.width - column * 0.5).abs() < px(1.0),
        "half the column, not {fill:?}"
    );
}
