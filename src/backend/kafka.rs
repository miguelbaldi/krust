use rdkafka::client::ClientContext;
use rdkafka::config::{ClientConfig, FromClientConfigAndContext, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{Consumer, ConsumerContext};
use rdkafka::error::KafkaResult;
use rdkafka::message::Headers;
use rdkafka::topic_partition_list::TopicPartitionList;
use rdkafka::Message;
use std::fmt::{self, Display};
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

use crate::backend::repository::{KrustConnection, KrustHeader, KrustMessage};

use super::repository::KrustConnectionSecurityType;

const TIMEOUT: Duration = Duration::from_millis(5000);

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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq)]
pub struct Partition {
    pub id: i32,
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq)]
pub struct Topic {
    pub name: String,
    pub partitions: Vec<Partition>,
}

impl Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone)]
pub struct KafkaBackend {
    pub config: KrustConnection,
}

impl KafkaBackend {
    pub fn new(config: KrustConnection) -> Self {
        Self { config: config }
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

    pub async fn list_topics(&self) -> Vec<Topic> {
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
                partitions.push(Partition { id: partition.id() });
            }

            topics.push(Topic {
                name: topic.name().to_string(),
                partitions: partitions,
            });
        }
        topics
    }

    pub fn topic_message_count(&self, topic: String) -> usize {
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        debug!("Consumer created");

        let metadata = consumer
            .fetch_metadata(Some(&topic.as_str()), TIMEOUT)
            .expect("Failed to fetch metadata");

        let mut message_count: usize = 0;
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
                    message_count += usize::try_from(high).unwrap() - usize::try_from(low).unwrap();
                }
            }
            None => warn!(""),
        }
        debug!("Topic {} has {} messages", topic, message_count);
        message_count
    }
    pub async fn list_messages_for_topic(&self, topic: String) -> Vec<KrustMessage> {
        let start_mark = Instant::now();
        let topic_name = topic.as_str();
        let context = CustomContext;
        let consumer: LoggingConsumer = self.consumer(context).expect("Consumer creation failed");

        debug!("Consumer created");
        let total = self.topic_message_count(topic.clone());
        let mut counter = 0;

        consumer
            .subscribe(&[topic_name])
            .expect("Can't subscribe to specified topics");

        let mut messages = vec![];

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
                    messages.push(KrustMessage {
                        id: None,
                        connection_id: None,
                        topic: m.topic().to_string(),
                        partition: m.partition(),
                        offset: m.offset(),
                        timestamp: m.timestamp().to_millis(),
                        value: payload.to_string(),
                        headers: headers,
                    });
                    counter += 1;
                    //consumer.commit_message(&m, CommitMode::Async).unwrap();
                }
            };
        }
        let duration = start_mark.elapsed();
        info!("duration: {:?}", duration);
        messages
    }
}
