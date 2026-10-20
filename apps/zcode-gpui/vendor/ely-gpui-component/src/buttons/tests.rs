use gpui::{
    Context, FocusHandle, IntoElement, KeyBinding, KeyUpEvent, Keystroke, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window,
};

use super::Button;
use crate::{
    primitives::{FocusNext, FocusScope},
    theme::Theme,
};

/// One button that counts its presses.
struct Bench {
    root: FocusHandle,
    loading: bool,
    presses: usize,
}

impl Render for Bench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        FocusScope::new(&self.root).root().size_full().child(
            Button::new("save", "Save")
                .loading(self.loading)
                .on_click(move |_, _, cx| view.update(cx, |bench, _| bench.presses += 1)),
        )
    }
}

fn press(key: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).expect("a key"),
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_loading_button_ignores_presses_and_keeps_its_tab_stop(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Theme::init(cx);
        cx.bind_keys([KeyBinding::new("tab", FocusNext, None)]);
    });
    let (view, cx) = cx.add_window_view(|_, cx| Bench {
        root: cx.focus_handle(),
        loading: true,
        presses: 0,
    });
    let root = view.read_with(cx, |bench, _| bench.root.clone());
    cx.update(|window, cx| window.focus(&root, cx));
    press("tab", cx);
    press("enter", cx);
    assert_eq!(view.read_with(cx, |bench, _| bench.presses), 0);
    view.update(cx, |bench, cx| {
        bench.loading = false;
        cx.notify();
    });
    cx.run_until_parked();
    press("enter", cx);
    assert_eq!(
        view.read_with(cx, |bench, _| bench.presses),
        1,
        "focus stayed on it"
    );
}
