use super::*;

fn item(content: &str, status: &str) -> PlanItem {
    PlanItem {
        id: content.into(),
        content: content.into(),
        status: status.into(),
    }
}

fn option(provider: &str, model: &str, name: &str) -> ModelOption {
    ModelOption {
        value: format!("{provider}/{model}"),
        name: name.into(),
        provider_name: provider.into(),
        thought_levels: Vec::new(),
        default_thought: String::new(),
        provider: provider.into(),
        model: model.into(),
    }
}

#[test]
fn model_label_prefers_the_catalog_name_and_falls_back_to_the_model_id() {
    let models = [
        option("anthropic", "claude-sonnet", "Claude Sonnet"),
        option("openrouter", "claude-sonnet", "Sonnet via OpenRouter"),
    ];
    assert_eq!(
        model_label("openrouter/claude-sonnet", &models),
        "Sonnet via OpenRouter"
    );
    assert_eq!(
        model_label("anthropic/claude-sonnet", &models),
        "Claude Sonnet"
    );
    assert_eq!(model_label("zai/glm-4.6", &models), "glm-4.6");
    assert_eq!(model_label("bare-model", &[]), "bare-model");
}

#[test]
fn todo_progress_points_at_the_active_then_next_pending_todo() {
    assert_eq!(todo_progress(&[]), None);
    let todos = [
        item("Read pane", "completed"),
        item("Write report", "pending"),
        item("Run tests", "inProgress"),
    ];
    assert_eq!(
        todo_progress(&todos),
        Some(TodoProgress {
            current: Some("Run tests".into()),
            completed: 1,
            total: 3,
        })
    );
    let pending = [
        item("Read pane", "completed"),
        item("Write report", "pending"),
    ];
    assert_eq!(
        todo_progress(&pending).and_then(|p| p.current),
        Some("Write report".into())
    );
    let done = [item("Read pane", "completed")];
    assert_eq!(
        todo_progress(&done),
        Some(TodoProgress {
            current: None,
            completed: 1,
            total: 1,
        })
    );
}

#[test]
fn summary_preview_takes_the_first_text_line_of_a_markdown_report() {
    assert_eq!(summary_preview(None, 10), None);
    assert_eq!(summary_preview(Some("  \n---\n"), 10), None);
    assert_eq!(
        summary_preview(Some("## Findings\n\nThe pane renders twice."), 80),
        Some("Findings".into())
    );
    assert_eq!(
        summary_preview(Some("\n- **Root cause**: `cx.notify`   loops"), 80),
        Some("Root cause: cx.notify loops".into())
    );
    assert_eq!(
        summary_preview(Some("This is a long summary text"), 14),
        Some("This is a long…".into())
    );
}
