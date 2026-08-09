use std::collections::HashSet;

use chrono::{DateTime, Utc};
use orion_domain::{
    events::ensure_event_compatible, EventEnvelope, EventId, NotificationId, NotificationKind,
    NotificationRequestedV1, Rating, RatingEntryId, RatingReason, RatingUpdatedV1, UserId,
    EVENT_CONTRACTS,
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
        serde_json::to_string_pretty(&rating).expect("serialize rating event"),
        include_str!("../../../docs/contracts/fixtures/rating_updated_v1.json").trim_end()
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
        serde_json::to_string_pretty(&notification).expect("serialize notification event"),
        include_str!("../../../docs/contracts/fixtures/notification_requested_v1.json").trim_end()
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
            docs.contains(&format!("`{}`", event.event_type)),
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
