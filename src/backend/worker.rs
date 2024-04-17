use chrono::Utc;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::Repository;

use super::{
    kafka::{KafkaBackend, KafkaFetch},
    repository::{KrustConnection, KrustMessage, KrustTopic, MessagesRepository},
};

#[derive(Debug, Clone, Copy, PartialEq, Default, strum::EnumString, strum::Display)]
pub enum MessagesMode {
    #[default]
    Live,
    Cached {
        refresh: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, strum::EnumString, strum::Display)]
pub enum PageOp {
    Next,
    Prev,
}

#[derive(Debug, Clone)]
pub struct MessagesRequest {
    pub mode: MessagesMode,
    pub connection: KrustConnection,
    pub topic: KrustTopic,
    pub page_operation: PageOp,
    pub page_size: u16,
    pub offset_partition: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct MessagesResponse {
    pub page_operation: PageOp,
    pub page_size: u16,
    pub total: usize,
    pub messages: Vec<KrustMessage>,
    pub topic: Option<KrustTopic>,
}

pub struct MessagesCleanupRequest {
    pub connection: KrustConnection,
    pub topic: KrustTopic,
}

pub struct MessagesWorker;

impl MessagesWorker {
    pub fn new() -> Self {
        MessagesWorker {}
    }

    pub fn cleanup_messages(self, request: &MessagesCleanupRequest) {
        let mut repo = Repository::new();
        let conn_id = request.connection.id.unwrap();
        let has_topic = repo.find_topic(conn_id, &request.topic.name);
        match has_topic {
            Some(topic) => {
                let mut mrepo = MessagesRepository::new(conn_id, &topic.name);
                let destroy_result = mrepo.destroy();
                match destroy_result {
                    Ok(_) => {
                        let destroy_result = repo.delete_topic(conn_id, &topic);
                        match destroy_result {
                            Ok(r) => info!("topic removed: {}", r),
                            Err(e) => warn!("unable to remove topic: {:?}", e),
                        }
                    }
                    Err(e) => {
                        warn!("unable to destroy cache: {:?}", e);
                    }
                }
            }
            None => info!("nothing to cleanup"),
        }
    }
    pub async fn get_messages(
        self,
        token: CancellationToken,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, String> {
        let req = request.clone();
        let join_handle = tokio::spawn(async move {
            // Wait for either cancellation or a very long time
            select! {
                _ = token.cancelled() => {
                    info!("request {:?} cancelled", &req);
                    // The token was cancelled
                    MessagesResponse { total: 0, messages: Vec::new(), topic: None, page_operation: req.page_operation.clone(), page_size: req.page_size}
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
            MessagesMode::Cached { refresh: _ } => self.get_messages_cached(request).await,
        }
    }
    async fn get_messages_cached(self, request: &MessagesRequest) -> MessagesResponse {
        let refresh = match request.mode {
            MessagesMode::Live => false,
            MessagesMode::Cached { refresh } => refresh,
        };
        let cached = if refresh {
            Some(Utc::now().timestamp_millis())
        } else {
            request.topic.cached.or(Some(Utc::now().timestamp_millis()))
        };
        let kafka = KafkaBackend::new(&request.connection);
        let mut repo = Repository::new();
        let topic_name = &request.topic.name;
        let topic = KrustTopic {
            connection_id: request.topic.connection_id.clone(),
            name: request.topic.name.clone(),
            cached: cached,
            partitions: vec![],
        };
        let topic = match repo.save_topic(
            topic.connection_id.expect("should have connection id"),
            &topic,
        ) {
            Ok(saved_topic) => saved_topic,
            Err(e) => {
                error!("problem saving topic: {:?}", e);
                topic
            }
        };
        // Run async background task
        let mut mrepo = MessagesRepository::new(topic.connection_id.unwrap(), &topic.name);
        let total = match request.topic.cached.clone() {
            Some(_) => {
                if refresh {
                    let cached_total = mrepo.count_messages().unwrap_or_default();
                    let total = kafka.topic_message_count(&topic.name).await - cached_total;
                    let partitions = mrepo.find_offsets().ok();
                    kafka
                    .list_messages_for_topic(
                        &topic_name,
                        total,
                        Some(&mut mrepo),
                        partitions,
                        Some(KafkaFetch::Newest),
                    )
                    .await;
                }
                mrepo.count_messages().unwrap_or_default()
            }
            None => {
                let total = kafka.topic_message_count(&topic.name).await;
                mrepo.init().unwrap();
                kafka
                    .list_messages_for_topic(
                        &topic_name,
                        total,
                        Some(&mut mrepo),
                        None,
                        Some(KafkaFetch::Oldest),
                    )
                    .await;
                total
            }
        };
        let messages = match request.page_operation {
            PageOp::Next => match request.offset_partition {
                (0, 0) => mrepo.find_messages(request.page_size).unwrap(),
                offset_partition => mrepo
                    .find_next_messages(request.page_size, offset_partition)
                    .unwrap(),
            },
            PageOp::Prev => mrepo
                .find_prev_messages(request.page_size, request.offset_partition)
                .unwrap(),
        };
        MessagesResponse {
            total,
            messages,
            topic: Some(topic),
            page_operation: request.page_operation,
            page_size: request.page_size,
        }
    }

    async fn get_messages_live(self, request: &MessagesRequest) -> MessagesResponse {
        let kafka = KafkaBackend::new(&request.connection);
        let topic = &request.topic.name;
        // Run async background task
        let total = kafka.topic_message_count(&topic).await;
        let messages = kafka
            .list_messages_for_topic(&topic, total, None, None, Some(KafkaFetch::Oldest))
            .await;
        MessagesResponse {
            total,
            messages,
            topic: Some(request.topic.clone()),
            page_operation: request.page_operation,
            page_size: request.page_size,
        }
    }
}
