use std::collections::HashMap;

use chrono::Utc;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{config::ExternalError, Repository};

use super::{
    kafka::{KafkaBackend, KafkaFetch},
    repository::{KrustConnection, KrustMessage, KrustTopic, MessagesRepository, Partition},
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
    pub search: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MessagesResponse {
    pub page_operation: PageOp,
    pub page_size: u16,
    pub total: usize,
    pub messages: Vec<KrustMessage>,
    pub topic: Option<KrustTopic>,
    pub search: Option<String>,
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
    ) -> Result<MessagesResponse, ExternalError> {
        let req = request.clone();
        let join_handle = tokio::spawn(async move {
            // Wait for either cancellation or a very long time
            select! {
                _ = token.cancelled() => {
                    info!("request {:?} cancelled", &req);
                    // The token was cancelled
                    Ok(MessagesResponse { total: 0, messages: Vec::new(), topic: Some(req.topic), page_operation: req.page_operation, page_size: req.page_size, search: req.search})
                }
                messages = self.get_messages_by_mode(&req) => {
                    messages
                }
            }
        });
        join_handle.await?
    }

    async fn get_messages_by_mode(
        self,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, ExternalError> {
        match request.mode {
            MessagesMode::Live => self.get_messages_live(request).await,
            MessagesMode::Cached { refresh: _ } => self.get_messages_cached(request).await,
        }
    }



    async fn get_messages_cached(
        self,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, ExternalError> {
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
            connection_id: request.topic.connection_id,
            name: request.topic.name.clone(),
            cached,
            partitions: vec![],
            total: None,
        };
        let topic = repo.save_topic(
            topic.connection_id.expect("should have connection id"),
            &topic,
        )?;
        // Run async background task
        let mut mrepo = MessagesRepository::new(topic.connection_id.unwrap(), &topic.name);
        let total = match request.topic.cached {
            Some(_) => {
                if refresh {
                    let partitions = mrepo.find_offsets().ok();
                    let topic = kafka.topic_message_count(&topic.name, partitions.clone()).await;
                    let partitions = topic.partitions.clone();
                    let total = topic.total.unwrap_or_default();
                    kafka
                    .cache_messages_for_topic(
                        topic_name,
                        total,
                        &mut mrepo,
                        Some(partitions),
                        Some(KafkaFetch::Newest),
                    )
                    .await
                    .unwrap();
                }
                mrepo
                .count_messages(request.search.clone())
                .unwrap_or_default()
            }
            None => {
                let total = kafka
                .topic_message_count(&topic.name, None)
                .await
                .total
                .unwrap_or_default();
                mrepo.init().unwrap();
                kafka
                .cache_messages_for_topic(
                    topic_name,
                    total,
                    &mut mrepo,
                    None,
                    Some(KafkaFetch::Oldest),
                )
                .await
                .unwrap();
                let total = if request.search.clone().is_some() {
                    mrepo
                    .count_messages(request.search.clone())
                    .unwrap_or_default()
                } else {
                    total
                };
                total
            }
        };
        let messages = match request.page_operation {
            PageOp::Next => match request.offset_partition {
                (0, 0) => mrepo
                .find_messages(request.clone().page_size, request.clone().search)
                .unwrap(),
                offset_partition => mrepo
                .find_next_messages(request.page_size, offset_partition, request.clone().search)
                .unwrap(),
            },
            PageOp::Prev => mrepo
            .find_prev_messages(
                request.page_size,
                request.offset_partition,
                request.clone().search,
            )
            .unwrap(),
        };
        Ok(MessagesResponse {
            total,
            messages,
            topic: Some(topic),
            page_operation: request.page_operation,
            page_size: request.page_size,
            search: request.search.clone(),
        })
    }

    async fn get_messages_live(
        self,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, ExternalError> {
        let kafka = KafkaBackend::new(&request.connection);
        let topic = &request.topic.name;
        // Run async background task
        let total = kafka
        .topic_message_count(topic, None)
        .await
        .total
        .unwrap_or_default();
        let messages = kafka.list_messages_for_topic(topic, total).await?;
        Ok(MessagesResponse {
            total,
            messages: messages,
            topic: Some(request.topic.clone()),
            page_operation: request.page_operation,
            page_size: request.page_size,
            search: request.search.clone(),
        })
    }
}
