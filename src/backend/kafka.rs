// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use futures::future;
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::{ClientContext, DefaultClientContext};
use rdkafka::config::{ClientConfig, FromClientConfigAndContext, RDKafkaLogLevel};
use rdkafka::consumer::BaseConsumer;
use rdkafka::consumer::{Consumer, ConsumerContext};
use rdkafka::error::{KafkaError, KafkaResult};
use rdkafka::message::{Header, Headers, OwnedHeaders};

use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::topic_partition_list::TopicPartitionList;
use rdkafka::{Message, Offset};
use tokio::select;
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

use crate::backend::repository::{KrustConnection, KrustHeader, KrustMessage, Partition};
use crate::component::task_manager::{Task, TaskManagerMsg, TASK_MANAGER_BROKER};
use crate::config::ExternalError;
use crate::Settings;

use super::repository::{
    FetchMode, KrustConnectionSecurityType, KrustTopic, KrustTopicCache, MessagesRepository,
};

const GROUP_ID: &str = "krust-kafka-client";

// rdkafka: begin

// A context can be used to change the behavior of producers and consumers by adding callbacks
// that will be executed by librdkafka.
// This particular context sets up custom callbacks to log rebalancing events.
struct CustomContext;

impl ClientContext for CustomContext {}

impl ConsumerContext for CustomContext {
    fn commit_callback(&self, result: KafkaResult<()>, _offsets: &TopicPartitionList) {
        info!("Committing offsets: {:?}", result);
    }
}

// A type alias with your custom consumer can be created for convenience.
type LoggingConsumer = BaseConsumer<CustomContext>;

// rdkafka: end

#[derive(Debug, Clone, Default, strum::EnumString, strum::Display)]
pub enum KafkaFetch {
    #[default]
    Newest,
    Oldest,
}

impl KafkaFetch {
    pub const VALUES: [Self; 2] = [Self::Newest, Self::Oldest];
}

#[derive(Debug, Clone)]
pub struct CreateTopicRequest {
    pub name: String,
    pub partition_count: u16,
    pub replica_count: u8,
}

#[derive(Debug, Clone)]
pub struct KafkaBackend {
    pub config: KrustConnection,
}

#[derive(Clone)]
pub struct CacheMessagesRequest<'a> {
    pub task: Task,
    pub cache_settings: KrustTopicCache,
    pub messages_repository: &'a MessagesRepository,
    pub refresh: bool,
}

impl KafkaBackend {
    pub fn new(config: &KrustConnection) -> Self {
        Self {
            config: config.clone(),
        }
    }
    fn timeout(&self) -> Duration {
        let default_timeout = Settings::read()
            .unwrap_or_default()
            .default_connection_timeout as u64;
        let timeout = Duration::from_secs(
            self.config
                .timeout
                .map(|t| t.try_into().unwrap_or(default_timeout))
                .unwrap_or(default_timeout),
        );
        info!("kafka::connection::timeout: {:?}", timeout);
        timeout
    }
    fn create_config(&self) -> ClientConfig {
        let mut config = ClientConfig::new();
        match self.config.security_type {
            KrustConnectionSecurityType::SASL_PLAINTEXT => {
                config
                    .set("bootstrap.servers", self.config.brokers_list.clone())
                    .set("group.id", GROUP_ID)
                    .set("enable.partition.eof", "false")
                    .set("session.timeout.ms", "6000")
                    .set("enable.auto.commit", "false")
                    .set("message.timeout.ms", "10000")
                    //.set("statistics.interval.ms", "30000")
                    .set("auto.offset.reset", "earliest")
                    .set("security.protocol", self.config.security_type.to_string())
                    .set(
                        "sasl.mechanisms",
                        self.config.sasl_mechanism.clone().unwrap_or_default(),
                    )
                    .set(
                        "sasl.username",
                        self.config.sasl_username.clone().unwrap_or_default(),
                    )
                    .set(
                        "sasl.password",
                        self.config.sasl_password.clone().unwrap_or_default(),
                    )
                    //.set("sasl.jaas.config", self.config.jaas_config.clone().unwrap_or_default())
                    .set_log_level(RDKafkaLogLevel::Debug)
            }
            _ => {
                config
                    .set("bootstrap.servers", self.config.brokers_list.clone())
                    .set("group.id", GROUP_ID)
                    .set("enable.partition.eof", "false")
                    .set("session.timeout.ms", "6000")
                    .set("enable.auto.commit", "false")
                    .set("message.timeout.ms", "10000")
                    //.set("statistics.interval.ms", "30000")
                    .set("auto.offset.reset", "earliest")
            }
        }
        .to_owned()
    }
    fn producer(&self) -> Result<FutureProducer, KafkaError> {
        self.create_config().create()
    }
    fn consumer<C, T>(&self, context: C) -> KafkaResult<T>
    where
        C: ClientContext,
        T: FromClientConfigAndContext<C>,
    {
        self.create_config().create_with_context(context)
    }
    fn create_admin_client(&self) -> Result<AdminClient<DefaultClientContext>, KafkaError> {
        self.create_config().create()
        //.expect("admin client creation failed")
    }

    pub async fn create_topic(self, request: &CreateTopicRequest) -> Result<bool, ExternalError> {
        let admin_client = self.create_admin_client()?;
        let opts = AdminOptions::new().operation_timeout(Some(self.timeout()));
        let topic = NewTopic::new(
            &request.name,
            request.partition_count as i32,
            TopicReplication::Fixed(request.replica_count as i32),
        );
        admin_client.create_topics(vec![&topic], &opts).await?;
        Ok(true)
    }

    pub async fn delete_topic(self, topic_name: String) -> Result<bool, ExternalError> {
        let admin_client = self.create_admin_client()?;
        let opts = AdminOptions::new().operation_timeout(Some(self.timeout()));
        admin_client
            .delete_topics(&[topic_name.as_str()], &opts)
            .await?;
        Ok(true)
    }

    pub async fn list_topics(&self) -> Result<Vec<KrustTopic>, ExternalError> {
        let context = CustomContext;
        let consumer: LoggingConsumer = self
            .consumer(context)
            .map_err(ExternalError::KafkaUnexpectedError)?;

        debug!("Consumer created");
        let metadata = consumer
            .fetch_metadata(None, self.timeout())
            .map_err(ExternalError::KafkaUnexpectedError)?;

        let mut topics = vec![];
        for topic in metadata.topics() {
            let mut partitions = vec![];
            for partition in topic.partitions() {
                partitions.push(Partition {
                    id: partition.id(),
                    offset_low: None,
                    offset_high: None,
                });
            }

            topics.push(KrustTopic {
                connection_id: self.config.id,
                name: topic.name().to_string(),
                cached: None,
                partitions,
                total: None,
                favourite: None,
            });
        }
        Ok(topics)
    }

    pub async fn fetch_partitions(&self, topic: &String) -> Vec<Partition> {
        info!("fetching partitions from topic {}", topic);
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        debug!("Consumer created");

        let metadata = consumer
            .fetch_metadata(Some(topic.as_str()), self.timeout())
            .expect("Failed to fetch metadata");

        let mut partitions = vec![];
        match metadata.topics().first() {
            Some(t) => {
                for partition in t.partitions() {
                    let (low, high) = consumer
                        .fetch_watermarks(t.name(), partition.id(), self.timeout())
                        .unwrap_or((-1, -1));
                    trace!(
                        "Low watermark: {}  High watermark: {} (difference: {})",
                        low,
                        high,
                        high - low
                    );
                    let part = Partition {
                        id: partition.id(),
                        offset_low: Some(low),
                        offset_high: Some(high),
                    };
                    partitions.push(part);
                }
            }
            None => warn!(""),
        }
        partitions
    }
    pub async fn send_messages(&self, topic: &String, messages: &[KrustMessage]) {
        info!("[send_messages] creating producer for topic {}", topic);
        let producer: FutureProducer = self.producer().expect("Producer creation failed");
        let producer = producer.borrow();

        debug!("[send_messages] producer created");
        let messages_futures = messages
            .iter()
            .map(|message| async move {
                // The send operation on the topic returns a future, which will be
                // completed once the result or failure from Kafka is received.
                let mut kheaders = OwnedHeaders::new();
                for h in message.headers.clone().iter() {
                    let value = h.value.as_ref();
                    let key = h.key.as_str();
                    let header = Header { key, value };
                    kheaders = kheaders.insert(header);
                }
                let delivery_status = producer
                    .send(
                        FutureRecord::to(topic)
                            .partition(message.partition)
                            .payload(&message.value)
                            .key(&message.key.clone().unwrap_or_default())
                            .headers(kheaders),
                        Duration::from_secs(0),
                    )
                    .await;

                // This will be executed when the result is received.
                trace!("Delivery status for message {:?} received", message);
                delivery_status
            })
            .collect::<Vec<_>>();
        // This loop will wait until all delivery statuses have been received.
        for future in messages_futures {
            let result = future.await;
            trace!("Message sent, future completed. Result: {}", result.is_ok());
        }
    }

    pub async fn topic_message_count(
        &self,
        topic: &String,
        fetch: Option<KafkaFetch>,
        max_messages: Option<i64>,
        current_partitions: Option<Vec<Partition>>,
    ) -> KrustTopic {
        info!(
            "couting messages for topic {}, fetch {:?}, max messages {:?}",
            topic, fetch, max_messages
        );

        let mut message_count: i64 = 0;
        let partitions = &self.fetch_partitions(topic).await;
        let mut result = current_partitions.clone().unwrap_or_default();
        let cpartitions = &current_partitions.unwrap_or_default().clone();
        let fetch = fetch.unwrap_or_default();
        let max_messages: i64 = max_messages.unwrap_or_default();

        let part_map = cpartitions
            .iter()
            .map(|p| (p.id, p))
            .collect::<HashMap<_, _>>();

        for p in partitions {
            if !cpartitions.is_empty() {
                let low = match part_map.get(&p.id) {
                    Some(part) => {
                        let o = part.offset_high.unwrap_or(p.offset_low.unwrap());
                        if o < p.offset_low.unwrap() {
                            p.offset_low.unwrap()
                        } else {
                            o
                        }
                    }
                    None => {
                        result.push(Partition {
                            id: p.id,
                            offset_low: p.offset_low,
                            offset_high: None,
                        });
                        p.offset_low.unwrap()
                    }
                };
                message_count += p.offset_high.unwrap_or_default() - low;
            } else {
                let (low, high) = match fetch {
                    KafkaFetch::Newest => {
                        let low = p.offset_high.unwrap_or(max_messages) - max_messages;
                        debug!(
                            "Newest::[low={},new_low={},high={},max={}]",
                            p.offset_low.unwrap_or_default(),
                            low,
                            p.offset_high.unwrap_or_default(),
                            max_messages
                        );
                        if max_messages > 0
                            && p.offset_high.unwrap_or_default() >= max_messages
                            && low >= p.offset_low.unwrap_or_default()
                        {
                            (low, p.offset_high.unwrap_or_default())
                        } else {
                            (
                                p.offset_low.unwrap_or_default(),
                                p.offset_high.unwrap_or_default(),
                            )
                        }
                    }
                    KafkaFetch::Oldest => {
                        let high = p.offset_low.unwrap_or_default() + max_messages;
                        debug!(
                            "Oldest::[low={},high={},new_high={},max={}]",
                            p.offset_low.unwrap_or_default(),
                            p.offset_high.unwrap_or_default(),
                            high,
                            max_messages
                        );
                        if max_messages > 0
                            && p.offset_low.unwrap_or_default() < high
                            && high <= p.offset_high.unwrap_or_default()
                        {
                            (p.offset_low.unwrap_or_default(), high)
                        } else {
                            (
                                p.offset_low.unwrap_or_default(),
                                p.offset_high.unwrap_or_default(),
                            )
                        }
                    }
                };
                result.push(Partition {
                    id: p.id,
                    offset_low: Some(low),
                    offset_high: Some(high),
                });
                message_count += high - low;
            };
        }

        let output = KrustTopic {
            connection_id: None,
            name: topic.clone(),
            cached: None,
            partitions: if !result.is_empty() {
                result
            } else {
                partitions.clone()
            },
            total: Some(
                message_count
                    .try_into()
                    .expect("should return the total messages as usize"),
            ),
            favourite: None,
        };
        info!(
            "topic {} has {} messages: {:?}",
            topic, message_count, &output.partitions
        );
        output
    }
    pub async fn fetch_and_build_partition_list<'a>(
        &self,
        request: &'a CacheMessagesRequest<'a>,
    ) -> TopicPartitionList {
        let topic = &request.cache_settings.topic_name.clone();
        let fetch = request.cache_settings.fetch_mode;
        let fetch_value = request.cache_settings.fetch_value.unwrap_or_default();
        let refresh = request.refresh;
        let mut mrepo = request.messages_repository.clone();
        info!(
            "fetching and building partitions for topic {}, fetch mode {:?}",
            topic, fetch,
        );

        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");
        let partitions = &self.fetch_partitions(topic).await;
        let mut partition_list = TopicPartitionList::with_capacity(partitions.len());
        if refresh {
            let cached_partitions = (mrepo).find_offsets().unwrap_or_default();
            let part_map = cached_partitions
                .iter()
                .map(|p| (p.id, p))
                .collect::<HashMap<_, _>>();
            partitions.iter().for_each(|p| {
                partition_list
                    .add_partition_offset(
                        topic,
                        p.id,
                        part_map
                            .get(&p.id)
                            .and_then(|cp| cp.offset_high)
                            .map(Offset::from_raw)
                            .unwrap_or(Offset::Beginning),
                    )
                    .expect("should add partition/offset to list");
            });
        } else {
            match fetch {
                FetchMode::All | FetchMode::Head => partitions.iter().for_each(|p| {
                    partition_list
                        .add_partition_offset(topic, p.id, Offset::Beginning)
                        .expect("should add partition/offset to list");
                }),
                FetchMode::Tail => partitions.iter().for_each(|p| {
                    partition_list
                        .add_partition_offset(topic, p.id, Offset::OffsetTail(fetch_value))
                        .expect("should add partition/offset to list");
                }),
                FetchMode::FromTimestamp => {
                    let mut tpl = TopicPartitionList::with_capacity(partitions.len());
                    partitions.iter().for_each(|p| {
                        tpl.add_partition_offset(topic, p.id, Offset::from_raw(fetch_value))
                            .expect("should add partition/offset to list");
                    });
                    let result = consumer.offsets_for_times(tpl, Duration::from_secs(60));
                    if let Ok(tpl) = result {
                        tpl.elements().iter().for_each(|t| {
                            partition_list
                                .add_partition_offset(t.topic(), t.partition(), t.offset())
                                .expect("should add partition/offset to list");
                        });
                    };
                }
            };
        }

        info!("topic {} partition list {:?}", topic, partition_list);
        partition_list
    }
    pub async fn cache_messages<'a>(
        &self,
        request: &'a CacheMessagesRequest<'a>,
    ) -> Result<Duration, ExternalError> {
        let start_mark = Instant::now();
        let fetch = request.cache_settings.fetch_mode;
        let fetch_value = request.cache_settings.fetch_value;
        let topic_name = request.cache_settings.topic_name.clone();
        let mrepo = request.messages_repository;
        let task = request.task.clone();
        let partitions = self.fetch_and_build_partition_list(request).await;
        let (total, partitions_list) = match fetch {
            FetchMode::All => {
                let result = self
                    .topic_message_count(&topic_name, None, None, None)
                    .await;
                info!("cache_messages[{:?}]::{:?}", fetch, &result.partitions);
                (result.total.unwrap_or_default(), result.partitions)
            }
            FetchMode::Head => {
                let result = self
                    .topic_message_count(&topic_name, Some(KafkaFetch::Oldest), fetch_value, None)
                    .await;
                (result.total.unwrap_or_default(), result.partitions)
            }
            FetchMode::Tail => {
                let result = self
                    .topic_message_count(&topic_name, Some(KafkaFetch::Newest), fetch_value, None)
                    .await;
                (result.total.unwrap_or_default(), result.partitions)
            }
            FetchMode::FromTimestamp => {
                let ts_partitions: Vec<Partition> = partitions
                    .elements()
                    .iter()
                    .map(|p| Partition {
                        id: p.partition(),
                        offset_high: p.offset().to_raw(),
                        offset_low: None,
                    })
                    .collect();
                let result = self
                    .topic_message_count(&topic_name, None, None, Some(ts_partitions.clone()))
                    .await;
                let parts: Vec<Partition> = self
                    .fetch_partitions(&topic_name)
                    .await
                    .iter()
                    .map(|fp| {
                        let p = ts_partitions.iter().find(|p| p.id == fp.id);
                        Partition {
                            id: fp.id,
                            offset_low: p.and_then(|p1| p1.offset_high),
                            offset_high: fp.offset_high,
                        }
                    })
                    .collect();
                debug!("from_timestamp partitions: {:?}", &parts);
                (result.total.unwrap_or_default(), parts)
            }
        };
        let part_last_offset_map = partitions_list
            .iter()
            .map(|p| (p.id, p.offset_high.unwrap_or_default()))
            .collect::<HashMap<_, _>>();
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");
        let consumer = Arc::new(consumer);
        consumer
            .assign(&partitions)
            .expect("Can't subscribe to partition list");
        let counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel::<KrustMessage>(32);
        let writer_id = "worker-0".to_string();
        let writer_counter = Arc::new(AtomicUsize::new(0));
        let writer_task = task.clone();
        let writer_repo = mrepo.clone();
        let writer_token = writer_task.token.clone().unwrap();
        let last_offset_map = Arc::new(part_last_offset_map.clone());
        let writer_handle = tokio::spawn(async move {
            select! {
                _ = writer_token.cancelled() => {
                    info!("writer-{}::request with task {:?} cancelled", writer_id.clone(), &writer_task);
                    TASK_MANAGER_BROKER.send(TaskManagerMsg::RemoveTask(writer_task.clone()));
                    // The token was cancelled
                }
                _result = KafkaBackend::db_writer_worker(
                    writer_id.clone(),
                    rx,
                    writer_task.clone(),
                    writer_counter,
                    writer_repo,
                    total,
                    last_offset_map,
                ) => {}
            }
        });
        let timeout = self.timeout();
        let mk_consumer = |worker_id: String| {
            let timeout = Arc::new(timeout);
            let consumer = consumer.clone();
            let mcounter = counter.clone();
            let token = task.token.clone().unwrap();
            let tx = tx.clone();
            let consumer_task = task.clone();
            let last_offset_map = Arc::new(part_last_offset_map.clone());
            tokio::spawn(async move {
                select! {
                    _ = token.cancelled() => {
                        info!("consumer-{}::request with task {:?} cancelled", worker_id.clone(), &consumer_task);
                        TASK_MANAGER_BROKER.send(TaskManagerMsg::RemoveTask(consumer_task.clone()));
                        // The token was cancelled
                    }
                    _result = KafkaBackend::consumer_worker(worker_id.clone(), timeout, tx, consumer, mcounter, total, last_offset_map) => {}
                }
            })
        };
        let max_threads = Settings::read()
            .map(|st| st.threads_number - 1)
            .unwrap_or(2) as usize;
        let num_partitions = partitions.count();
        let parallelism_factor = if num_partitions <= max_threads {
            num_partitions
        } else {
            max_threads
        };
        info!(
            "cache_messages::starting consumers with parallelism factor of {}",
            parallelism_factor
        );
        for res in
            future::join_all((0..parallelism_factor).map(|i| mk_consumer(format!("worker-{}", i))))
                .await
        {
            res.unwrap();
        }
        std::mem::drop(tx);
        match writer_handle.await {
            Err(e) => {
                let duration = start_mark.elapsed();
                let seconds = duration.as_secs() % 60;
                let minutes = (duration.as_secs() / 60) % 60;
                let hours = (duration.as_secs() / 60) / 60;
                let msg = format!(
                    "error caching messages for topic {}, duration: {}:{}:{}: {}",
                    topic_name, hours, minutes, seconds, e
                );
                core::result::Result::Err(ExternalError::CachingError(topic_name.clone(), msg))
            }
            Ok(_) => {
                let duration = start_mark.elapsed();
                info!(
                    "finished caching messages for topic {}, duration: {:?}",
                    topic_name, duration
                );
                core::result::Result::Ok(duration)
            }
        }
    }

    async fn db_writer_worker(
        worker_id: String,
        mut rx: Receiver<KrustMessage>,
        task: Task,
        counter: Arc<AtomicUsize>,
        repo: MessagesRepository,
        total: usize,
        part_last_offset_map: Arc<HashMap<i32, i64>>,
    ) {
        info!("Starting writer-{} total[{}]", worker_id, total);
        let conn = repo.get_connection();
        // Start receiving messages
        while let Some(message) = rx.recv().await {
            trace!(
                "writer-{}::message with offset {} trying to save",
                worker_id,
                &message.offset
            );
            match repo.save_message(&conn, &message) {
                Ok(_) => {
                    trace!(
                        "writer-{}::message with offset {} saved",
                        worker_id,
                        &message.offset
                    );
                }
                Err(err) => warn!(
                    "writer-{}::unable to save message with offset {}: {}",
                    worker_id,
                    &message.offset,
                    err.to_string()
                ),
            };
            let _previous_count = counter.fetch_add(1, Ordering::SeqCst);
            let current_count = counter.load(Ordering::SeqCst);
            let progress_step = ((current_count as f64) * 1.0) / ((total as f64) * 1.0);
            TASK_MANAGER_BROKER.send(TaskManagerMsg::Progress(task.clone(), progress_step));
            let current_offset = message.offset;
            let current_partition = message.partition;
            let max_offset = *part_last_offset_map
                .get(&current_partition)
                .expect("should have partition last offset");
            trace!(
                "writer-{}::{}/{} [partition={}, offset={}, max_offset={}]",
                worker_id,
                current_count,
                total,
                current_partition,
                current_offset,
                max_offset
            );
        }
        info!("writer-{} finished", worker_id);
    }
    async fn consumer_worker(
        worker_id: String,
        timeout: Arc<Duration>,
        tx: Sender<KrustMessage>,
        consumer: Arc<BaseConsumer<CustomContext>>,
        mcounter: Arc<AtomicUsize>,
        total: usize,
        part_last_offset_map: Arc<HashMap<i32, i64>>,
    ) {
        let timeout = timeout.mul_f32(3.0);
        info!("Starting consumer-{}::timeout::{:?}", worker_id, timeout);
        let local_counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        loop {
            match consumer.poll(timeout) {
                None => {
                    warn!("consumer-{} timeout", worker_id);
                    break;
                }
                Some(result) => match result {
                    Err(e) => warn!("Kafka Error: {}", e),
                    Ok(m) => {
                        let current_offset = m.offset();
                        let current_partition = m.partition();
                        let max_offset = *part_last_offset_map
                            .get(&current_partition)
                            .expect("should have partition last offset");
                        if current_offset < max_offset {
                            let payload = match m.payload_view::<str>() {
                                None => "",
                                Some(Ok(s)) => s,
                                Some(Err(e)) => {
                                    warn!("Error while deserializing message payload: {:?}", e);
                                    ""
                                }
                            };
                            let key = match m.key_view::<str>() {
                                None => "",
                                Some(Ok(s)) => s,
                                Some(Err(e)) => {
                                    warn!("Error while deserializing message key: {:?}", e);
                                    ""
                                }
                            };
                            trace!("message received: topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                                m.topic(),
                                m.partition(),
                                m.offset(),
                                m.timestamp());
                            let headers = if let Some(headers) = m.headers() {
                                let mut header_list: Vec<KrustHeader> = vec![];
                                for header in headers.iter() {
                                    let h = KrustHeader {
                                        key: header.key.to_string(),
                                        value: header.value.map(|v| {
                                            String::from_utf8(v.to_vec()).unwrap_or_default()
                                        }),
                                    };
                                    header_list.push(h);
                                }
                                header_list
                            } else {
                                vec![]
                            };
                            let message = KrustMessage {
                                topic: m.topic().to_string(),
                                partition: m.partition(),
                                offset: m.offset(),
                                key: Some(key.to_string()),
                                timestamp: m.timestamp().to_millis(),
                                value: payload.to_string(),
                                headers,
                            };
                            match tx.send(message).await {
                                Err(e) => warn!(
                                    "consumer-{}::Problem sending message to writer: offset={}, {}",
                                    worker_id,
                                    m.offset(),
                                    e
                                ),
                                Ok(_) => trace!(
                                    "consumer-{}::Message sent to writer: offset={}",
                                    worker_id,
                                    m.offset()
                                ),
                            };
                            let _local = local_counter.fetch_add(1, Ordering::SeqCst);
                            let local = local_counter.load(Ordering::SeqCst);
                            let _num = mcounter.fetch_add(1, Ordering::SeqCst);
                            let num = mcounter.load(Ordering::SeqCst);

                            trace!(
                                "consumer-{}::{}/{} [partition={}, offset={}, max_offset={}]",
                                worker_id,
                                num,
                                total,
                                current_partition,
                                current_offset,
                                max_offset
                            );
                            if num >= total {
                                info!("consumer-{}::done::{}", worker_id, local);
                                break;
                            }
                        } else {
                            let local = local_counter.load(Ordering::SeqCst);
                            let global = mcounter.load(Ordering::SeqCst);
                            trace!(
                                "consumer-{}::larger_offset::[partition={}, offset={}/{}, local={}, global={}]",
                                worker_id, current_partition, current_offset, max_offset, local, global
                            );
                            if global >= total {
                                debug!(
                                    "consumer-{}::done(larger_offset)::[partition={}, offset={}/{}, local={}, global={}]",
                                    worker_id, current_partition, current_offset, max_offset, local, global
                                );
                                break;
                            }
                        }
                    }
                },
            };
        }
        let c_val = local_counter.load(Ordering::SeqCst);
        info!("consumer-{} finished: {}", worker_id, c_val);
        std::mem::drop(tx);
    }
    pub async fn list_messages_for_topic(
        &self,
        task: Task,
        topic: &String,
        fetch: Option<KafkaFetch>,
        max_messages: Option<i64>,
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let start_mark = Instant::now();
        info!("starting listing messages for topic {}", topic);
        let topic_name = topic.as_str();
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        let mut counter = 0;

        let topic = self
            .topic_message_count(topic, fetch.clone(), max_messages, None)
            .await;
        let total = topic.total.unwrap_or_default();
        let partitions = topic.partitions.clone();

        let max_offset_map = partitions
            .clone()
            .into_iter()
            .map(|p| (p.id, p.offset_high.unwrap_or_default()))
            .collect::<HashMap<_, _>>();

        let mut partition_list = TopicPartitionList::with_capacity(partitions.capacity());
        for p in partitions.iter() {
            let offset = Offset::from_raw(p.offset_low.unwrap_or_default());
            partition_list
                .add_partition_offset(topic_name, p.id, offset)
                .unwrap();
        }
        info!("seeking partitions\n{:?}", partition_list);
        consumer
            .assign(&partition_list)
            .expect("Can't subscribe to partition list");

        let mut messages: Vec<KrustMessage> = Vec::with_capacity(total);
        while counter < total {
            match consumer.poll(Duration::from_secs(5)) {
                None => warn!("Kafka timeout"),
                Some(result) => match result {
                    Err(e) => warn!("Kafka error: {}", e),
                    Ok(m) => {
                        let max_offset = match max_offset_map.get(&m.partition()) {
                            Some(max) => *max,
                            None => 0,
                        };
                        if m.offset() <= max_offset {
                            let payload = match m.payload_view::<str>() {
                                None => "",
                                Some(Ok(s)) => s,
                                Some(Err(e)) => {
                                    warn!("Error while deserializing message payload: {:?}", e);
                                    ""
                                }
                            };
                            let key = match m.key_view::<str>() {
                                None => "",
                                Some(Ok(s)) => s,
                                Some(Err(e)) => {
                                    warn!("Error while deserializing message key: {:?}", e);
                                    ""
                                }
                            };
                            trace!("key: '{:?}', payload: '{}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                                key, payload, m.topic(), m.partition(), m.offset(), m.timestamp());
                            let headers = if let Some(headers) = m.headers() {
                                let mut header_list: Vec<KrustHeader> = vec![];
                                for header in headers.iter() {
                                    let h = KrustHeader {
                                        key: header.key.to_string(),
                                        value: header.value.map(|v| {
                                            String::from_utf8(v.to_vec()).unwrap_or_default()
                                        }),
                                    };
                                    header_list.push(h);
                                }
                                header_list
                            } else {
                                vec![]
                            };
                            let message = KrustMessage {
                                topic: m.topic().to_string(),
                                partition: m.partition(),
                                offset: m.offset(),
                                key: Some(key.to_string()),
                                timestamp: m.timestamp().to_millis(),
                                value: payload.to_string(),
                                headers,
                            };

                            messages.push(message);
                            counter += 1;
                            let progress_step = ((counter as f64) * 1.0) / ((total as f64) * 1.0);
                            TASK_MANAGER_BROKER
                                .send(TaskManagerMsg::Progress(task.clone(), progress_step));
                        }
                    }
                },
            };
        }
        let duration = start_mark.elapsed();
        info!(
            "finished listing messages for topic {}, duration: {:?}",
            topic, duration
        );
        Ok(messages)
    }
}
