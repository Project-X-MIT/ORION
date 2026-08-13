use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
    routing::get,
    Router,
};
use tokio::sync::broadcast;

use crate::{
    routes::auth::AuthenticatedUser, state::AppState, websocket::events::NotificationEvent,
};

#[derive(Clone)]
pub struct NotificationGateway {
    sender: broadcast::Sender<NotificationEvent>,
}

impl Default for NotificationGateway {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(128);
        Self { sender }
    }
}

impl NotificationGateway {
    pub fn publish(&self, event: NotificationEvent) {
        let _ = self.sender.send(event);
    }
    fn subscribe(&self) -> broadcast::Receiver<NotificationEvent> {
        self.sender.subscribe()
    }

    fn event_for_user(event: &NotificationEvent, user_id: uuid::Uuid) -> bool {
        event.recipient_id == user_id
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/ws/notifications", get(connect))
}

async fn connect(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| serve(socket, auth.user.id, state.notification_gateway))
}

async fn serve(mut socket: WebSocket, user_id: uuid::Uuid, gateway: NotificationGateway) {
    let mut receiver = gateway.subscribe();
    while let Ok(event) = receiver.recv().await {
        if !NotificationGateway::event_for_user(&event, user_id) {
            continue;
        }
        let Ok(payload) = serde_json::to_string(&event) else {
            continue;
        };
        if socket.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationEvent, NotificationGateway};
    use tokio::sync::broadcast::error::RecvError;
    use uuid::Uuid;

    #[tokio::test]
    async fn gateway_only_delivers_events_for_the_authenticated_recipient() {
        let gateway = NotificationGateway::default();
        let mut receiver = gateway.subscribe();
        let user_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        gateway.publish(NotificationEvent {
            notification_id: Uuid::new_v4(),
            recipient_id: other_id,
        });
        gateway.publish(NotificationEvent {
            notification_id: Uuid::new_v4(),
            recipient_id: user_id,
        });

        let first = receiver.recv().await.expect("receive first event");
        let second = receiver.recv().await.expect("receive second event");
        assert!(!NotificationGateway::event_for_user(&first, user_id));
        assert!(NotificationGateway::event_for_user(&second, user_id));
    }

    #[tokio::test]
    async fn slow_subscriber_is_bounded_and_reports_backpressure() {
        let gateway = NotificationGateway::default();
        let mut receiver = gateway.subscribe();
        for _ in 0..129 {
            gateway.publish(NotificationEvent {
                notification_id: Uuid::new_v4(),
                recipient_id: Uuid::new_v4(),
            });
        }
        assert!(matches!(receiver.recv().await, Err(RecvError::Lagged(skipped)) if skipped > 0));
    }
}
