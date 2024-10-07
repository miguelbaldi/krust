// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use chrono::Utc;
use tokio::select;
use tracing::*;

use crate::{
    component::{
        messages::messages_page::{MessagesPageMsg, MESSAGES_PAGE_BROKER},
        task_manager::{Task, TaskManagerMsg, TASK_MANAGER_BROKER},
    },
    config::ExternalError,
    Repository,
};

use super::{
    kafka::{CacheMessagesRequest, KafkaBackend, KafkaFetch},
    repository::{
        FetchMode, KrustConnection, KrustMessage, KrustTopic, KrustTopicCache, MessagesRepository,
        MessagesSearchOrder,
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
    pub cache: Option<KrustTopicCache>,
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
    pub connection_id: usize,
    pub topic_name: String,
    pub refresh: bool,
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
        let conn_id = request.connection_id;
        let mut mrepo = MessagesRepository::new(conn_id, &request.topic_name);
        let destroy_result = mrepo.destroy();
        match destroy_result {
            Ok(_) => info!("cache destroyed on disk"),
            Err(e) => warn!("unable to destroy cache on disk: {:?}", e),
        };
        let has_topic = repo.find_topic(conn_id, &request.topic_name);
        match has_topic {
            Some(topic) => {
                let save_result = repo.delete_topic_cache(conn_id, topic.name.clone());
                match save_result {
                    Ok(r) => {
                        info!("topic cache destroyed: {}", r);
                        if request.refresh {
                            MESSAGES_PAGE_BROKER.send(MessagesPageMsg::RefreshTopicTab {
                                connection_id: conn_id,
                                topic_name: topic.name.clone(),
                            });
                        }
                        repo.find_topic(conn_id, &topic.name)
                    }
                    Err(e) => {
                        warn!("unable to update topic: {:?}", e);
                        None
                    }
                }
            }
            None => {
                info!("nothing to cleanup on database");
                None
            }
        }
    }
    pub async fn count_messages(self, request: &MessagesTotalCounterRequest) -> Option<usize> {
        let kafka = KafkaBackend::new(&request.connection);
        let mtopic = kafka
            .topic_message_count(&request.topic.name, Some(KafkaFetch::Oldest), None, None)
            .await;

        let total = mtopic.total.unwrap_or_default();
        Some(total)
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
        let topic = request.topic.clone();
        let cached = request.cache.clone();
        let refresh = match request.mode {
            MessagesMode::Live => false,
            MessagesMode::Cached { refresh } => refresh,
        };
        let cached_ts = if refresh {
            Some(Utc::now().timestamp_millis())
        } else {
            cached
                .clone()
                .map(|c| c.last_updated.unwrap_or(Utc::now().timestamp_millis()))
                .or(Some(Utc::now().timestamp_millis()))
        };
        let cached = if let Some(cached) = cached {
            KrustTopicCache {
                connection_id: cached.connection_id,
                topic_name: cached.topic_name,
                fetch_mode: cached.fetch_mode,
                fetch_value: cached.fetch_value,
                default_page_size: cached.default_page_size,
                last_updated: cached_ts,
            }
        } else {
            KrustTopicCache {
                connection_id: request.connection.id.unwrap(),
                topic_name: topic.name.clone(),
                fetch_mode: FetchMode::default(),
                fetch_value: None,
                default_page_size: 0,
                last_updated: cached_ts,
            }
        };
        let kafka = KafkaBackend::new(&request.connection);
        let mut repo = Repository::new();

        let topic_name = &request.topic.name;
        let current_cache = repo.find_topic_cache(
            topic.connection_id.expect("should have connection id"),
            topic_name,
        );

        // Run async background task
        let mut mrepo = MessagesRepository::new(topic.connection_id.unwrap(), &topic.name);
        let total = match current_cache {
            Some(_) => {
                if refresh {
                    let cache_request = CacheMessagesRequest {
                        cache_settings: cached.clone(),
                        task: task.clone(),
                        messages_repository: &mrepo,
                        refresh: true,
                    };
                    kafka.cache_messages(&cache_request).await.unwrap();
                    info!("cache refreshed");
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
                    let cache_request = CacheMessagesRequest {
                        cache_settings: cached.clone(),
                        task: task.clone(),
                        messages_repository: &mrepo,
                        refresh: false,
                    };
                    kafka.cache_messages(&cache_request).await.unwrap();
                }
                mrepo
                    .count_messages(request.search.clone())
                    .unwrap_or_default()
            }
        };

        let save_cache_result =
            repo.save_topic_cache(topic.connection_id.unwrap(), topic.name.clone(), &cached);
        match save_cache_result {
            Ok(_) => info!("topic cache settings saved!"),
            Err(e) => error!("error saving topic cache settings: {}", e),
        }
        let topic = repo
            .find_topic(
                topic.connection_id.expect("should have connection id"),
                topic_name,
            )
            .unwrap();
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
