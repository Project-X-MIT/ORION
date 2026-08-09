use serde::Serialize;

/// Implemented by every event payload that crosses a feature boundary.
/// Event type and version are compile-time constants so producers cannot emit
/// an unversioned payload accidentally.
pub trait VersionedEvent: Serialize {
    const EVENT_TYPE: &'static str;
    const SCHEMA_VERSION: u16;
}
