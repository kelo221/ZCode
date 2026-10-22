//! Tool results that only echo what the card already renders from the input
//! (desktop parity: plan and todo cards show the plan, not the result echo).
use serde_json::Value;

/// The cleaned `text` of a `tool` (canonical name) result as the card shows it.
pub(crate) fn displayed_result(tool: &str, text: String) -> String {
    match tool {
        "ExitPlanMode" => plan_result_status(&text).to_string(),
        // 原因：output_view 对所有工具打印原始结果，TodoWrite 的 JSON 回显（oldTodos/todos/summary）
        // 被整段印在清单下方。依据 desktop todo.tsx：解析出计划时不挂载 ToolOutput。
        "TodoWrite" if is_todo_result_echo(&text) => String::new(),
        _ => text,
    }
}

/// An approved ExitPlanMode result repeats the whole plan after its status
/// line (apps/zcode-cli/packages/core/src/tool/handlers/plan-mode.ts); the card
/// already renders the plan from the input, so only the status is kept.
/// Rejections and other results carry no echo and stay whole.
pub(crate) fn plan_result_status(text: &str) -> &str {
    match text.find("\n## Approved Plan:") {
        Some(at) => text[..at].trim_end(),
        None => text,
    }
}

/// A TodoWrite result echoes the list as JSON (`oldTodos`/`todos`/`summary`).
/// The card already renders the checklist; any other result, such as an
/// error, is not an echo.
pub(crate) fn is_todo_result_echo(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|v| v.get("todos").is_some_and(Value::is_array))
}
