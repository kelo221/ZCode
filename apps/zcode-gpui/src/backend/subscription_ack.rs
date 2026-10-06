use crate::app::store::AppState;
use serde_json::Value;

impl AppState {
    pub(crate) fn capture_subscription(
        &mut self,
        ws_key: &str,
        topic: &str,
        result: &Option<Value>,
    ) {
        let ack = result.as_ref().and_then(|r| r.get("ack"));
        let sub = ack
            .and_then(|a| a.get("subscriptionId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let epoch = ack
            .and_then(|a| a.get("logEpoch"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(sub) = sub.filter(|s| !s.is_empty()) {
            let full_topic = match topic {
                "sessions-index" | "workspace-config" => {
                    let key = self.ws(ws_key).map(|w| w.key.clone()).unwrap_or_default();
                    format!("{topic}/{key}")
                }
                other => other.to_string(),
            };
            // 订阅代次变化时必须丢弃旧游标和分片，恢复快照才不会被旧状态拒绝。
            let replaced = self
                .ws(ws_key)
                .and_then(|w| w.subscriptions.get(&full_topic))
                .map(|old| old.id != sub || old.log_epoch != epoch)
                .unwrap_or(true);
            if let Some(ws) = self.ws_mut(ws_key) {
                ws.subscriptions.insert(
                    full_topic.clone(),
                    crate::backend::workspace::RouteSubscription {
                        id: sub,
                        log_epoch: epoch,
                    },
                );
            }
            if replaced {
                self.route_cursors.remove(&full_topic);
                self.assembler.forget_topics(&[full_topic]);
            }
        }
    }
}
