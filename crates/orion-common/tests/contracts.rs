use std::collections::{BTreeMap, HashSet};

use orion_common::{
    ApiError, ApiFailure, ApiSuccess, ErrorCode, PageRequest, RequestId, API_OPERATIONS,
    CONFIG_KEYS,
};
use serde_json::json;
use uuid::Uuid;

fn fixture(name: &str) -> &'static str {
    match name {
        "success" => include_str!("../../../docs/contracts/fixtures/api_success_v1.json"),
        "failure" => include_str!("../../../docs/contracts/fixtures/api_error_v1.json"),
        _ => panic!("unknown fixture"),
    }
}

#[test]
fn api_envelopes_match_golden_fixtures() {
    let request_id = RequestId::from_uuid(
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid UUID"),
    );
    let success = ApiSuccess::new(
        request_id,
        json!({
            "service": "orion",
            "status": "ok"
        }),
    );
    assert_eq!(
        serde_json::to_string_pretty(&success).expect("serialize success"),
        fixture("success").trim_end()
    );

    let mut details = BTreeMap::new();
    details.insert("username".to_owned(), "must not be empty".to_owned());
    let failure = ApiFailure::new(
        request_id,
        ApiError {
            code: ErrorCode::ValidationFailed,
            message: "request validation failed".to_owned(),
            details,
        },
    );
    assert_eq!(
        serde_json::to_string_pretty(&failure).expect("serialize failure"),
        fixture("failure").trim_end()
    );
}

#[test]
fn api_and_configuration_registries_are_unique_owned_and_documented() {
    let api_docs = include_str!("../../../docs/contracts/api.md");
    let mut operation_ids = HashSet::new();
    for operation in API_OPERATIONS {
        assert!(operation_ids.insert(operation.operation_id));
        assert!(!operation.owner.is_empty());
        assert_eq!(operation.response_version, 1);
        assert!(
            api_docs.contains(&format!("`{}`", operation.operation_id)),
            "undocumented API operation: {}",
            operation.operation_id
        );
    }

    let config_docs = include_str!("../../../docs/contracts/configuration.md");
    let mut config_keys = HashSet::new();
    for key in CONFIG_KEYS {
        assert!(config_keys.insert(key.key));
        assert!(!key.owner.is_empty());
        assert!(!key.environments.is_empty());
        assert!(
            config_docs.contains(&format!("`{}`", key.key)),
            "undocumented configuration key: {}",
            key.key
        );
    }
}

#[test]
fn pagination_rejects_unbounded_requests() {
    assert!(PageRequest::new(0, 0).is_err());
    assert!(PageRequest::new(101, 0).is_err());
    assert_eq!(PageRequest::new(100, 40).expect("valid page").offset, 40);
}
