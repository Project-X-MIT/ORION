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
        if event.recipient_id != user_id {
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
