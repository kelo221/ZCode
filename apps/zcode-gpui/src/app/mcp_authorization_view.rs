use crate::app::root::RootView;
use crate::backend::mcp_authorization::McpAuthorizationReceipt;
use crate::shared::i18n::label;
use ely_gpui_component::buttons::Button;
#[cfg(not(test))]
use gpui::AppContext;
use gpui::{AnyElement, Context, IntoElement, ParentElement, div};

impl RootView {
    pub(crate) fn mcp_authorization_button(
        &self,
        server: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let receipt = self.state.read(cx).mcp_authorization_receipt(server)?;
        let id = format!("mcp-authorize-{server}");
        let button = Button::new(id.clone(), label("Open authorization", "打开授权页面")).on_click(
            cx.listener(move |this, _, _, cx| {
                this.dispatch_mcp_authorization(&receipt, cx);
            }),
        );
        let wrapper = div().child(button);
        #[cfg(test)]
        let wrapper = crate::app::test_support::track_children(wrapper, vec![id]);
        Some(wrapper.into_any_element())
    }

    fn dispatch_mcp_authorization(
        &mut self,
        receipt: &McpAuthorizationReceipt,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        self.state
            .read(cx)
            .open_mcp_authorization(receipt, |url| cx.open_url(url));
        #[cfg(not(test))]
        {
            let mut url = None;
            self.state
                .read(cx)
                .open_mcp_authorization(receipt, |value| url = Some(value.to_string()));
            let Some(url) = url else {
                return;
            };
            let state = self.state.clone();
            let receipt = receipt.clone();
            cx.spawn(async move |_, cx| {
                let result = cx
                    .background_spawn(async move { crate::shared::os::sensitive_url::open(&url) })
                    .await;
                if result.is_err() {
                    state.update(cx, |s, cx| {
                        s.mcp_authorization_open_failed(&receipt);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
    }
}
