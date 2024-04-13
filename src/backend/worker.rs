use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::{
    kafka::KafkaBackend,
    repository::{KrustConnection, KrustMessage},
};

#[derive(Debug, Clone, Copy, PartialEq, Default, strum::EnumString, strum::Display)]
pub enum MessagesMode {
    #[default]
    Live,
    Cached,
}

#[derive(Debug, Clone)]
pub struct MessagesRequest {
    pub mode: MessagesMode,
    pub connection: KrustConnection,
    pub topic_name: String,
}

#[derive(Debug, Clone)]
pub struct MessagesResponse {
    pub total: usize,
    pub messages: Vec<KrustMessage>,
}

pub struct MessagesWorker;

impl MessagesWorker {
    pub fn new() -> Self {
        MessagesWorker {}
    }

    pub async fn get_messages(self, token: CancellationToken, request: &MessagesRequest) -> Result<MessagesResponse, String> {
        let req = request.clone();
        let join_handle = tokio::spawn(async move {
            // Wait for either cancellation or a very long time
            select! {
                _ = token.cancelled() => {
                    info!("request {:?} cancelled", &req);
                    // The token was cancelled
                    MessagesResponse { total: 0, messages: Vec::new(), }
                }
                messages = self.get_messages_by_mode(&req) => {
                    messages
                }
            }
        });
        join_handle.await.map_err(|e| e.to_string())
    }

    async fn get_messages_by_mode(self, request: &MessagesRequest) -> MessagesResponse {
        match request.mode {
            MessagesMode::Live => self.get_messages_live(request).await,
            MessagesMode::Cached => todo!(),
        }
    }
    async fn get_messages_live(self, request: &MessagesRequest) -> MessagesResponse {
        let kafka = KafkaBackend::new(&request.connection);
        let topic = &request.topic_name;
        // Run async background task
        let total = kafka.topic_message_count(&topic);
        let messages = kafka.list_messages_for_topic(&topic, total).await;
        MessagesResponse { total, messages }
    }
}
