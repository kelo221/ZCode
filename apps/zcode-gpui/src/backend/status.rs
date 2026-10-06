use crate::backend::workspace::WorkspaceHandle;

pub(crate) fn set_workspace_error(
    workspaces: &mut [WorkspaceHandle],
    key: &str,
    message: &str,
) -> String {
    let message = crate::shared::redact::scrub(message);
    // 响应属于发起请求的工作区；切换后的活动工作区不能接收其错误状态。
    if let Some(workspace) = workspaces.iter_mut().find(|w| w.key == key) {
        workspace.status = format!("error: {message}");
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_error_changes_only_origin_and_is_redacted() {
        let mut a = WorkspaceHandle::new(std::env::temp_dir().join("a"), vec![]);
        let b = WorkspaceHandle::new(std::env::temp_dir().join("b"), vec![]);
        a.status = "connected".into();
        let key = b.key.clone();
        let mut workspaces = vec![a, b];
        let message = set_workspace_error(
            &mut workspaces,
            &key,
            "Authorization: Bearer sentinel-secret",
        );
        assert_eq!(workspaces[0].status, "connected");
        assert!(workspaces[1].status.starts_with("error:"));
        assert!(!workspaces[1].status.contains("sentinel-secret"));
        assert!(!message.contains("sentinel-secret"));
    }
}
