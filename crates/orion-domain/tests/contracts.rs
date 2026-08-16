use std::collections::HashSet;

use chrono::{DateTime, Utc};
use orion_domain::{
    events::ensure_event_compatible, AdvancedCacheInvalidationRequestedV1, AdvancedRatingEventV1,
    AdvancedSettlementCompletedV1, AdvancedSettlementDeadLetteredV1, AdvancedSubmissionRequestedV1,
    EventEnvelope, EventId, NotificationId, NotificationKind, NotificationRequestedV1, Rating,
    RatingEntryId, RatingReason, RatingUpdatedV1, UserId, EVENT_CONTRACTS,
};
use uuid::Uuid;

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("valid fixture UUID")
}

fn occurred_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-09T12:00:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc)
}

fn normalize_fixture_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

#[test]
fn versioned_events_match_golden_fixtures() {
    let rating = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000010")),
        occurred_at(),
        "orion-worker",
        RatingUpdatedV1 {
            rating_entry_id: RatingEntryId::from_uuid(uuid("00000000-0000-0000-0000-000000000011")),
            user_id: UserId::from_uuid(uuid("00000000-0000-0000-0000-000000000012")),
            previous_rating: Rating::new(1500).expect("valid rating"),
            current_rating: Rating::new(1516).expect("valid rating"),
            delta: 16,
            reason: RatingReason::BasicQuiz,
            source_id: uuid("00000000-0000-0000-0000-000000000013"),
        },
    );
    assert_eq!(
        normalize_fixture_newlines(
            &serde_json::to_string_pretty(&rating).expect("serialize rating event"),
        ),
        normalize_fixture_newlines(
            include_str!("../../../docs/contracts/fixtures/rating_updated_v1.json").trim_end(),
        )
    );

    let notification = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000020")),
        occurred_at(),
        "orion-api",
        NotificationRequestedV1 {
            notification_id: NotificationId::from_uuid(uuid(
                "00000000-0000-0000-0000-000000000021",
            )),
            recipient_id: UserId::from_uuid(uuid("00000000-0000-0000-0000-000000000012")),
            kind: NotificationKind::RatingChanged,
            title: "Rating updated".to_owned(),
            body: "Your rating increased by 16.".to_owned(),
            action_url: Some("/profile".to_owned()),
            deduplication_key: "rating-entry:00000000-0000-0000-0000-000000000011".to_owned(),
        },
    );
    assert_eq!(
        normalize_fixture_newlines(
            &serde_json::to_string_pretty(&notification).expect("serialize notification event"),
        ),
        normalize_fixture_newlines(
            include_str!("../../../docs/contracts/fixtures/notification_requested_v1.json")
                .trim_end(),
        )
    );
}

#[test]
fn advanced_events_match_golden_fixtures() {
    let submitted = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000040")),
        DateTime::parse_from_rfc3339("2026-08-16T12:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc),
        "orion-api",
        AdvancedSubmissionRequestedV1 {
            attempt_id: uuid("00000000-0000-0000-0000-000000000041"),
            user_id: uuid("00000000-0000-0000-0000-000000000042"),
            question_ids: vec![uuid("00000000-0000-0000-0000-000000000043")],
            dedupe_key: "advanced-submission:00000000-0000-0000-0000-000000000041".to_owned(),
        },
    );
    assert_fixture(&submitted, "advanced_submitted_v1.json");

    let settled = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000050")),
        DateTime::parse_from_rfc3339("2026-08-16T12:01:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc),
        "orion-worker",
        AdvancedSettlementCompletedV1 {
            attempt_id: uuid("00000000-0000-0000-0000-000000000041"),
            user_id: uuid("00000000-0000-0000-0000-000000000042"),
            status: "completed".to_owned(),
            rating_after: 1530,
            events: vec![AdvancedRatingEventV1 {
                event_id: uuid("00000000-0000-0000-0000-000000000051"),
                question_id: uuid("00000000-0000-0000-0000-000000000043"),
                correct: true,
                rating_delta: 30,
            }],
            dedupe_key: "advanced-settlement:00000000-0000-0000-0000-000000000041".to_owned(),
        },
    );
    assert_fixture(&settled, "advanced_settled_v1.json");

    let cache = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000060")),
        DateTime::parse_from_rfc3339("2026-08-16T12:01:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc),
        "orion-worker",
        AdvancedCacheInvalidationRequestedV1 {
            attempt_id: uuid("00000000-0000-0000-0000-000000000041"),
            user_id: uuid("00000000-0000-0000-0000-000000000042"),
            question_ids: vec![uuid("00000000-0000-0000-0000-000000000043")],
            dedupe_key: "advanced-settlement:00000000-0000-0000-0000-000000000041:cache".to_owned(),
        },
    );
    assert_fixture(&cache, "advanced_cache_invalidate_v1.json");

    let dead_lettered = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000070")),
        DateTime::parse_from_rfc3339("2026-08-16T12:05:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc),
        "orion-worker",
        AdvancedSettlementDeadLetteredV1 {
            attempt_id: uuid("00000000-0000-0000-0000-000000000041"),
            user_id: uuid("00000000-0000-0000-0000-000000000042"),
            reason: "provider_terminal_failure".to_owned(),
            dedupe_key: "advanced-settlement:00000000-0000-0000-0000-000000000041:dead-letter"
                .to_owned(),
        },
    );
    assert_fixture(&dead_lettered, "advanced_dead_lettered_v1.json");
}

fn assert_fixture<T: serde::Serialize>(event: &EventEnvelope<T>, filename: &str) {
    let fixture = match filename {
        "advanced_submitted_v1.json" => {
            include_str!("../../../docs/contracts/fixtures/advanced_submitted_v1.json")
        }
        "advanced_settled_v1.json" => {
            include_str!("../../../docs/contracts/fixtures/advanced_settled_v1.json")
        }
        "advanced_cache_invalidate_v1.json" => {
            include_str!("../../../docs/contracts/fixtures/advanced_cache_invalidate_v1.json")
        }
        "advanced_dead_lettered_v1.json" => {
            include_str!("../../../docs/contracts/fixtures/advanced_dead_lettered_v1.json")
        }
        _ => panic!("unknown Advanced event fixture {filename}"),
    };
    assert_eq!(
        normalize_fixture_newlines(
            &serde_json::to_string_pretty(event).expect("serialize event fixture"),
        ),
        normalize_fixture_newlines(fixture.trim_end()),
        "fixture {filename} does not match the serialized contract"
    );
}

#[test]
fn event_registry_is_unique_owned_documented_and_versioned() {
    let docs = include_str!("../../../docs/contracts/events.md");
    let mut event_types = HashSet::new();
    for event in EVENT_CONTRACTS {
        assert!(event_types.insert(event.event_type));
        assert!(!event.owner.is_empty());
        assert!(event.current_version > 0);
        assert!(event.minimum_supported_version > 0);
        assert!(event.minimum_supported_version <= event.current_version);
        assert!(
            docs.contains(&format!(
                "| `{}` | {} | {} |",
                event.event_type, event.owner, event.current_version
            )),
            "undocumented event: {}",
            event.event_type
        );
    }
}

#[test]
fn compatibility_gate_rejects_unknown_or_unversioned_events() {
    assert!(ensure_event_compatible("orion.rating.updated", 1).is_ok());
    assert!(ensure_event_compatible("orion.rating.updated", 0).is_err());
    assert!(ensure_event_compatible("orion.rating.updated", 2).is_err());
    assert!(ensure_event_compatible("orion.unknown", 1).is_err());
}

#[test]
fn envelope_validation_rejects_tampered_contract_metadata() {
    let mut envelope = EventEnvelope::new(
        EventId::from_uuid(uuid("00000000-0000-0000-0000-000000000030")),
        occurred_at(),
        "orion-api",
        NotificationRequestedV1 {
            notification_id: NotificationId::from_uuid(uuid(
                "00000000-0000-0000-0000-000000000031",
            )),
            recipient_id: UserId::from_uuid(uuid("00000000-0000-0000-0000-000000000032")),
            kind: NotificationKind::System,
            title: "System".to_owned(),
            body: "System event".to_owned(),
            action_url: None,
            deduplication_key: "system:contract-test".to_owned(),
        },
    );
    assert!(envelope.validate_contract().is_ok());

    envelope.schema_version = 2;
    assert!(envelope.validate_contract().is_err());
}
