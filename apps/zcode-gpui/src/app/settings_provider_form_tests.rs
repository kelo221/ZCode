use super::*;
use crate::provider_settings::ProviderView;
use serde_json::json;

fn providers() -> ProviderView {
    ProviderView::from_value(&crate::provider_settings::tests::sample_view()).unwrap()
}

#[test]
fn custom_provider_fields_commit_only_real_changes() {
    let view = providers();
    let custom = view.provider("c1").unwrap();
    assert_eq!(field_update(custom, Field::Name, " First "), None);
    assert_eq!(field_update(custom, Field::Name, "  "), None);
    assert_eq!(
        field_update(custom, Field::Name, "Renamed"),
        Some(json!({ "type": "updateProvider", "providerId": "c1", "providerName": "Renamed" }))
    );
    assert_eq!(
        field_update(custom, Field::BaseUrl, "https://one.test "),
        None
    );
    assert_eq!(
        field_update(custom, Field::BaseUrl, " https://two.test "),
        Some(
            json!({ "type": "updateProvider", "providerId": "c1", "baseUrl": "https://two.test" })
        )
    );
    assert_eq!(
        field_update(custom, Field::ApiKey, "sk-2"),
        Some(json!({ "type": "updateProvider", "providerId": "c1", "apiKey": "sk-2" }))
    );
    assert_eq!(field_update(custom, Field::ApiKey, "sk-1"), None);
}

#[test]
fn preset_and_account_providers_never_send_read_only_fields() {
    let view = providers();
    let account = view.provider("zai").unwrap();
    assert_eq!(field_update(account, Field::Name, "Other"), None);
    assert_eq!(
        field_update(account, Field::BaseUrl, "https://x.test"),
        None
    );
    assert_eq!(field_update(account, Field::ApiKey, "k"), None);
}
