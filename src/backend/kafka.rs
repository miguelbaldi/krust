use rdkafka::client::ClientContext;
use rdkafka::config::{ClientConfig, FromClientConfigAndContext, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{Consumer, ConsumerContext};
use rdkafka::error::KafkaResult;
use rdkafka::message::Headers;
use rdkafka::topic_partition_list::TopicPartitionList;
use rdkafka::{Message, Offset};

use std::borrow::Borrow;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

use crate::backend::repository::{KrustConnection, KrustHeader, KrustMessage, Partition};
use crate::config::ExternalError;

use super::repository::{KrustConnectionSecurityType, KrustTopic, MessagesRepository};

const TIMEOUT: Duration = Duration::from_secs(240);

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
type LoggingConsumer = StreamConsumer<CustomContext>;

// rdkafka: end

#[derive(Debug, Clone, Default)]
pub enum KafkaFetch {
    #[default]
    Newest,
    Oldest,
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

    pub async fn list_topics(&self) -> Vec<KrustTopic> {
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        trace!("Consumer created");

        let metadata = consumer
            .fetch_metadata(None, TIMEOUT)
            .expect("Failed to fetch metadata");

        let mut topics = vec![];
        for topic in metadata.topics() {
            let mut partitions = vec![];
            for partition in topic.partitions() {
                // let (low, high) = consumer
                //         .fetch_watermarks(topic.name(), partition.id(), Duration::from_secs(1))
                //         .map(|(l,h)| (Some(l), Some(h)))
                //         .unwrap_or((None, None));
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
            });
        }
        topics
    }

    pub async fn fetch_partitions(&self, topic: &String) -> Vec<Partition> {
        info!("fetching partitions from topic {}", topic);
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        debug!("Consumer created");

        let metadata = consumer
            .fetch_metadata(Some(topic.as_str()), TIMEOUT)
            .expect("Failed to fetch metadata");

        let mut partitions = vec![];
        match metadata.topics().first() {
            Some(t) => {
                for partition in t.partitions() {
                    let (low, high) = consumer
                        .fetch_watermarks(t.name(), partition.id(), Duration::from_secs(1))
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

    pub async fn topic_message_count(
        &self,
        topic: &String,
        current_partitions: Option<Vec<Partition>>,
    ) -> KrustTopic {
        info!("couting messages for topic {}", topic);

        let mut message_count: usize = 0;
        let partitions = &self.fetch_partitions(topic).await;
        let mut result = current_partitions.clone().unwrap_or_default();
        let cpartitions = &current_partitions.unwrap_or_default().clone();

        let part_map = cpartitions
            .into_iter()
            .map(|p| (p.id, p.clone()))
            .collect::<HashMap<_, _>>();

        for p in partitions {
            if !cpartitions.is_empty() {
                let low = match part_map.get(&p.id) {
                    Some(part) => part.offset_high.unwrap_or(p.offset_low.unwrap()),
                    None => {
                        result.push(Partition {
                            id: p.id,
                            offset_low: p.offset_low,
                            offset_high: None,
                        });
                        p.offset_low.unwrap()
                    }
                };
                message_count += usize::try_from(p.offset_high.unwrap_or_default()).unwrap()
                    - usize::try_from(low).unwrap();
            } else {
                message_count += usize::try_from(p.offset_high.unwrap_or_default()).unwrap()
                    - usize::try_from(p.offset_low.unwrap_or_default()).unwrap();
            };
        }

        info!("topic {} has {} messages", topic, message_count);
        KrustTopic {
            connection_id: None,
            name: topic.clone(),
            cached: None,
            partitions: if !cpartitions.is_empty() {
                result
            } else {
                partitions.clone()
            },
            total: Some(message_count),
        }
    }
    pub async fn cache_messages_for_topic(
        &self,
        topic: &String,
        total: usize,
        mrepo: &mut MessagesRepository,
        partitions: Option<Vec<Partition>>,
        fetch: Option<KafkaFetch>,
    ) -> Result<Duration, ExternalError> {
        let start_mark = Instant::now();
        let fetch = fetch.unwrap_or_default();
        info!("starting listing messages for topic {}", topic);
        let topic_name = topic.as_str();
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");
        let conn = mrepo.get_connection();
        let mut counter = 0;

        info!("consumer created");
        match partitions {
            Some(partitions) => {
                let mut partition_list = TopicPartitionList::with_capacity(partitions.capacity());
                for p in partitions.iter() {
                    let offset = match fetch {
                        KafkaFetch::Newest => p
                            .offset_high
                            .map(|oh| Offset::from_raw(oh + 1))
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

        while counter < total {
            match consumer.recv().await {
                Err(e) => warn!("Kafka error: {}", e),
                Ok(m) => {
                    let payload = match m.payload_view::<str>() {
                        None => "",
                        Some(Ok(s)) => s,
                        Some(Err(e)) => {
                            warn!("Error while deserializing message payload: {:?}", e);
                            ""
                        }
                    };
                    trace!("key: '{:?}', payload: '{}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                    m.key(), payload, m.topic(), m.partition(), m.offset(), m.timestamp());
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
                        timestamp: m.timestamp().to_millis(),
                        value: payload.to_string(),
                        headers,
                    };
                    trace!("saving message {}", &message.offset);
                    mrepo.save_message(&conn, &message)?;
                    counter += 1;
                }
            };
        }
        let duration = start_mark.elapsed();
        info!(
            "finished listing messages for topic {}, duration: {:?}",
            topic, duration
        );
        core::result::Result::Ok(duration)
    }
    pub async fn list_messages_for_topic(
        &self,
        topic: &String,
        total: usize,
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let start_mark = Instant::now();
        info!("starting listing messages for topic {}", topic);
        let topic_name = topic.as_str();
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        let mut counter = 0;

        info!("consumer created");
        consumer
            .subscribe(&[topic_name])
            .expect("Can't subscribe to specified topics");
        let mut messages: Vec<KrustMessage> = Vec::with_capacity(total);
        while counter < total {
            match consumer.recv().await {
                Err(e) => warn!("Kafka error: {}", e),
                Ok(m) => {
                    let payload = match m.payload_view::<str>() {
                        None => "",
                        Some(Ok(s)) => s,
                        Some(Err(e)) => {
                            warn!("Error while deserializing message payload: {:?}", e);
                            ""
                        }
                    };
                    trace!("key: '{:?}', payload: '{}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                    m.key(), payload, m.topic(), m.partition(), m.offset(), m.timestamp());
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
                        timestamp: m.timestamp().to_millis(),
                        value: payload.to_string(),
                        headers,
                    };

                    messages.push(message);
                    counter += 1;
                }
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
