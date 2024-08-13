use futures::future;
use rdkafka::client::ClientContext;
use rdkafka::config::{ClientConfig, FromClientConfigAndContext, RDKafkaLogLevel};
use rdkafka::consumer::BaseConsumer;
use rdkafka::consumer::{Consumer, ConsumerContext};
use rdkafka::error::{KafkaError, KafkaResult};
use rdkafka::message::Headers;

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

use super::repository::{KrustConnectionSecurityType, KrustTopic, MessagesRepository};

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
pub struct KafkaBackend {
    pub config: KrustConnection,
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
    fn producer(&self) -> Result<FutureProducer, KafkaError> {
        let producer: Result<FutureProducer, KafkaError> = match self.config.security_type {
            KrustConnectionSecurityType::SASL_PLAINTEXT => {
                ClientConfig::new()
                    .set("bootstrap.servers", self.config.brokers_list.clone())
                    .set("group.id", GROUP_ID)
                    .set("enable.partition.eof", "false")
                    .set("session.timeout.ms", "6000")
                    .set("enable.auto.commit", "false")
                    .set("message.timeout.ms", "5000")
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
                    .create()
            }
            _ => {
                ClientConfig::new()
                    .set("bootstrap.servers", self.config.brokers_list.clone())
                    .set("group.id", GROUP_ID)
                    .set("enable.partition.eof", "false")
                    .set("session.timeout.ms", "6000")
                    .set("enable.auto.commit", "false")
                    .set("message.timeout.ms", "5000")
                    //.set("statistics.interval.ms", "30000")
                    .set("auto.offset.reset", "earliest")
                    .create()
            }
        };
        producer
    }
    fn consumer<C, T>(&self, context: C) -> KafkaResult<T>
    where
        C: ClientContext,
        T: FromClientConfigAndContext<C>,
    {
        match self.config.security_type {
            KrustConnectionSecurityType::SASL_PLAINTEXT => {
                ClientConfig::new()
                    .set("bootstrap.servers", self.config.brokers_list.clone())
                    .set("group.id", GROUP_ID)
                    .set("enable.partition.eof", "false")
                    .set("session.timeout.ms", "6000")
                    .set("enable.auto.commit", "false")
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
                    //.set("debug", "all")
                    .set_log_level(RDKafkaLogLevel::Debug)
                    .create_with_context::<C, T>(context)
            }
            _ => {
                ClientConfig::new()
                    .set("bootstrap.servers", self.config.brokers_list.clone())
                    .set("group.id", GROUP_ID)
                    .set("enable.partition.eof", "false")
                    .set("session.timeout.ms", "6000")
                    .set("enable.auto.commit", "false")
                    //.set("statistics.interval.ms", "30000")
                    .set("auto.offset.reset", "earliest")
                    .create_with_context::<C, T>(context)
            }
        }
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
        info!("fetching partitions from topic {}", topic);
        let producer: FutureProducer = self.producer().expect("Producer creation failed");
        let producer = producer.borrow();

        debug!("Producer created");
        let messages_futures = messages
            .iter()
            .map(|message| async move {
                // The send operation on the topic returns a future, which will be
                // completed once the result or failure from Kafka is received.
                let delivery_status = producer
                    .send(
                        FutureRecord::to(topic)
                            .partition(message.partition)
                            .payload(&message.value)
                            .key(&message.key.clone().unwrap_or_default()),
                        // .headers(OwnedHeaders::new().insert(Header {
                        //     key: "header_key",
                        //     value: Some("header_value"),
                        // })),
                        Duration::from_secs(0),
                    )
                    .await;

                // This will be executed when the result is received.
                info!("Delivery status for message {:?} received", message);
                delivery_status
            })
            .collect::<Vec<_>>();
        // This loop will wait until all delivery statuses have been received.
        for future in messages_futures {
            let result = future.await;
            info!("Future completed. Result: {:?}", result);
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

        info!("topic {} has {} messages", topic, message_count);
        KrustTopic {
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
        }
    }
    pub async fn cache_messages_for_topic(
        &self,
        task: Task,
        topic: &String,
        total: usize,
        mrepo: &MessagesRepository,
        partitions: Option<Vec<Partition>>,
        fetch: Option<KafkaFetch>,
    ) -> Result<Duration, ExternalError> {
        let start_mark = Instant::now();
        let fetch = fetch.unwrap_or_default();
        info!(
            "starting listing messages for topic {}, total {}",
            topic, total
        );
        let topic_name = topic.as_str();
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");
        let counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let consumer = Arc::new(consumer);
        info!("consumer created");
        match partitions.clone() {
            Some(partitions) => {
                let mut partition_list = TopicPartitionList::with_capacity(partitions.capacity());
                for p in partitions.iter() {
                    let offset = match fetch {
                        KafkaFetch::Newest => p
                            .offset_high
                            .map(Offset::from_raw)
                            .unwrap_or(Offset::Beginning),
                        KafkaFetch::Oldest => Offset::Beginning,
                    };
                    partition_list
                        .add_partition_offset(topic_name, p.id, offset)
                        .unwrap();
                }
                info!("seeking partitions\n{:?}", partition_list);
                consumer
                    .assign(&partition_list)
                    .expect("Can't subscribe to partition list");
            }
            None => {
                info!("consuming without seek");
                consumer
                    .subscribe(&[topic_name])
                    .expect("Can't subscribe to specified topics");
            }
        };
        let (tx, rx) = mpsc::channel::<KrustMessage>(32);
        let writer_id = "worker-0".to_string();
        let writer_counter = Arc::new(AtomicUsize::new(0));
        let writer_task = task.clone();
        let writer_repo = mrepo.clone();
        let writer_token = writer_task.token.clone().unwrap();
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
                ) => {}
            }
        });
        let mk_consumer = |worker_id: String| {
            let consumer = consumer.clone();
            let mcounter = counter.clone();
            let token = task.token.clone().unwrap();
            let tx = tx.clone();
            let consumer_task = task.clone();
            tokio::spawn(async move {
                select! {
                    _ = token.cancelled() => {
                        info!("consumer-{}::request with task {:?} cancelled", worker_id.clone(), &consumer_task);
                        TASK_MANAGER_BROKER.send(TaskManagerMsg::RemoveTask(consumer_task.clone()));
                        // The token was cancelled
                    }
                    _result = KafkaBackend::consumer_worker(worker_id.clone(), tx, consumer, mcounter, total) => {}
                }
            })
        };
        for res in future::join_all((0..10).map(|i| mk_consumer(format!("worker-{}", i)))).await {
            res.unwrap();
        }
        match writer_handle.await {
            Err(e) => {
                let duration = start_mark.elapsed();
                let seconds = duration.as_secs() % 60;
                let minutes = (duration.as_secs() / 60) % 60;
                let hours = (duration.as_secs() / 60) / 60;
                let msg = format!(
                    "error caching messages for topic {}, duration: {}:{}:{}: {}",
                    topic, hours, minutes, seconds, e
                );
                core::result::Result::Err(ExternalError::CachingError(topic.clone(), msg))
            }
            Ok(_) => {
                let duration = start_mark.elapsed();
                info!(
                    "finished caching messages for topic {}, duration: {:?}",
                    topic, duration
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
    ) {
        let conn = repo.get_connection();
        // Start receiving messages
        while let Some(message) = rx.recv().await {
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
            trace!("writer-{}::{}/{}", worker_id, current_count, total);
            if current_count >= total {
                break;
            }
        }
        info!("writer-{} finished", worker_id);
    }
    async fn consumer_worker(
        worker_id: String,
        tx: Sender<KrustMessage>,
        consumer: Arc<BaseConsumer<CustomContext>>,
        mcounter: Arc<AtomicUsize>,
        total: usize,
    ) {
        info!("Starting consumer[{}]", worker_id);
        loop {
            trace!("[{}] waiting", worker_id);
            match consumer.poll(Duration::from_secs(5)) {
                None => {
                    warn!("[{}] timeout", worker_id);
                    break;
                }
                Some(result) => match result {
                    Err(e) => warn!("Kafka Error: {}", e),
                    Ok(m) => {
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
                        trace!(
                            "message received: topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                            m.topic(),
                            m.partition(),
                            m.offset(),
                            m.timestamp()
                        );
                        let headers = if let Some(headers) = m.headers() {
                            let mut header_list: Vec<KrustHeader> = vec![];
                            for header in headers.iter() {
                                let h = KrustHeader {
                                    key: header.key.to_string(),
                                    value: header
                                        .value
                                        .map(|v| String::from_utf8(v.to_vec()).unwrap_or_default()),
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
                        let _num = mcounter.fetch_add(1, Ordering::SeqCst);
                        let num = mcounter.load(Ordering::SeqCst);
                        trace!("consumer-{}::{}/{}", worker_id, num, total);
                        if num >= total {
                            break;
                        }
                    }
                },
            };
        }
        info!("{} finished", worker_id)
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
