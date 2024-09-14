use chrono::Utc;
use tokio::select;
use tracing::{info, warn};

use crate::{
    component::task_manager::{Task, TaskManagerMsg, TASK_MANAGER_BROKER},
    config::ExternalError,
    Repository,
};

use super::{
    kafka::{KafkaBackend, KafkaFetch},
    repository::{
        KrustConnection, KrustMessage, KrustTopic, MessagesRepository, MessagesSearchOrder,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Default, strum::EnumString, strum::Display)]
pub enum MessagesMode {
    #[default]
    Live,
    Cached {
        refresh: bool,
    },
}

#[derive(Debug, Clone)]
pub struct MessagesRequest {
    pub task: Option<Task>,
    pub mode: MessagesMode,
    pub connection: KrustConnection,
    pub topic: KrustTopic,
    pub page_size: u16,
    pub page: usize,
    pub search_order: Option<MessagesSearchOrder>,
    pub search: Option<String>,
    pub fetch: KafkaFetch,
    pub max_messages: i64,
}

#[derive(Debug, Clone)]
pub struct MessagesResponse {
    pub task: Option<Task>,
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

pub struct MessagesTotalCounterRequest {
    pub connection: KrustConnection,
    pub topic: KrustTopic,
}

pub struct MessagesWorker;

impl MessagesWorker {
    pub fn new() -> Self {
        MessagesWorker {}
    }

    pub fn cleanup_messages(self, request: &MessagesCleanupRequest) -> Option<KrustTopic> {
        let mut repo = Repository::new();
        let conn_id = request.connection.id.unwrap();
        let has_topic = repo.find_topic(conn_id, &request.topic.name);
        match has_topic {
            Some(mut topic) => {
                let mut mrepo = MessagesRepository::new(conn_id, &topic.name);
                let destroy_result = mrepo.destroy();
                topic.cached = None;
                match destroy_result {
                    Ok(_) => {
                        let save_result = repo.save_topic(conn_id, &topic);
                        match save_result {
                            Ok(r) => info!("topic updated: {}", r),
                            Err(e) => warn!("unable to update topic: {:?}", e),
                        };
                        Some(topic)
                    }
                    Err(e) => {
                        warn!("unable to destroy cache: {:?}", e);
                        Some(topic)
                    }
                }
            }
            None => {
                info!("nothing to cleanup");
                None
            }
        }
    }
    pub async fn count_messages(self, request: &MessagesTotalCounterRequest) -> Option<usize> {
        let mut repo = Repository::new();
        let conn_id = request.connection.id.unwrap();
        let has_topic = repo.find_topic(conn_id, &request.topic.name);
        let kafka = KafkaBackend::new(&request.connection);
        match has_topic {
            Some(topic) => {
                let mtopic = kafka
                    .topic_message_count(&topic.name, Some(KafkaFetch::Oldest), None, None)
                    .await;

                let total = mtopic.total.unwrap_or_default();
                Some(total)
            }
            None => {
                info!("nothing to count");
                None
            }
        }
    }
    pub async fn get_messages(
        self,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, ExternalError> {
        let task = request.task.clone();
        let token = request.task.clone().unwrap().token.unwrap();
        let req = request.clone();
        let join_handle = tokio::spawn(async move {
            // Wait for either cancellation or a very long time
            select! {
                _ = token.cancelled() => {
                    info!("request with task {:?} cancelled", &req.task);
                    TASK_MANAGER_BROKER.send(TaskManagerMsg::RemoveTask(task.unwrap()));
                    // The token was cancelled
                    Ok(MessagesResponse {
                        task: req.task,
                        total: 0,
                        messages: Vec::new(),
                        topic: Some(req.topic),
                        page_size: req.page_size,
                        search: req.search})
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
        let task = request.task.clone().unwrap();
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
            favourite: request.topic.favourite,
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
                    let topic = kafka
                        .topic_message_count(&topic.name, None, None, partitions.clone())
                        .await;
                    let partitions = topic.partitions.clone();
                    let total = topic.total.unwrap_or_default();
                    info!("cache refresh [total={}]", total);
                    kafka
                        .cache_messages_for_topic(
                            task.clone(),
                            topic_name,
                            total,
                            &mrepo,
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
                let mtopic = kafka
                    .topic_message_count(&topic.name, Some(KafkaFetch::Oldest), None, None)
                    .await;

                let total = mtopic.total.unwrap_or_default();

                mrepo.init().unwrap();
                if total > 0 {
                    kafka
                        .cache_messages_for_topic(
                            task.clone(),
                            topic_name,
                            total,
                            &mrepo,
                            Some(mtopic.partitions),
                            Some(KafkaFetch::Oldest),
                        )
                        .await
                        .unwrap();
                }
                if request.search.clone().is_some() {
                    mrepo
                        .count_messages(request.search.clone())
                        .unwrap_or_default()
                } else {
                    total
                }
            }
        };
        let messages = mrepo
            .find_messages_paged(
                task.clone(),
                request.page,
                request.page_size,
                request.search_order.clone(),
                request.search.clone(),
            )
            .unwrap();
        TASK_MANAGER_BROKER.send(TaskManagerMsg::Progress(task.clone(), 1.0));
        Ok(MessagesResponse {
            task: Some(task),
            total,
            messages,
            topic: Some(topic),
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
        let task = request.task.clone().unwrap();
        TASK_MANAGER_BROKER.send(TaskManagerMsg::Progress(task.clone(), 0.01));
        // Run async background task
        let messages = kafka
            .list_messages_for_topic(
                task.clone(),
                topic,
                Some(request.fetch.clone()),
                Some(request.max_messages),
            )
            .await?;
        if messages.is_empty() {
            TASK_MANAGER_BROKER.send(TaskManagerMsg::Progress(task.clone(), 1.0));
        }
        Ok(MessagesResponse {
            task: Some(task),
            total: messages.len(),
            messages,
            topic: Some(request.topic.clone()),
            page_size: request.page_size,
            search: request.search.clone(),
        })
    }
}
