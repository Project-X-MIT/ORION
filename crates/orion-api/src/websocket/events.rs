use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub notification_id: Uuid,
    pub recipient_id: Uuid,
}
