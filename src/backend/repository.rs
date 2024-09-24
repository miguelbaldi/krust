use std::fmt;
use std::path::PathBuf;
use std::string::ToString;
use std::{fmt::Display, str::FromStr};

use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{named_params, params, Connection, Row};
use serde::{Deserialize, Serialize};
use strum::EnumString;
use tracing::*;

use crate::component::task_manager::{Task, TaskManagerMsg, TASK_MANAGER_BROKER};
use crate::config::{
    database_connection, database_connection_with_name, destroy_database_with_name, ExternalError,
};

use super::settings::Settings;

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, PartialEq, Default, EnumString, strum::Display)]
pub enum KrustConnectionSecurityType {
    #[default]
    PLAINTEXT,
    SASL_PLAINTEXT,
}

impl KrustConnectionSecurityType {
    pub const VALUES: [Self; 2] = [Self::PLAINTEXT, Self::SASL_PLAINTEXT];
}

#[derive(Debug, Clone, Default)]
pub struct KrustConnection {
    pub id: Option<usize>,
    pub name: String,
    pub brokers_list: String,
    pub security_type: KrustConnectionSecurityType,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    pub color: Option<String>,
    pub timeout: Option<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq)]
pub struct Partition {
    pub id: i32,
    pub offset_low: Option<i64>,
    pub offset_high: Option<i64>,
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq)]
pub struct KrustTopic {
    pub connection_id: Option<usize>,
    pub name: String,
    pub cached: Option<KrustTopicCache>,
    pub partitions: Vec<Partition>,
    pub total: Option<usize>,
    pub favourite: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq)]
pub enum FetchMode {
    #[default]
    All,
    Tail,
    Head,
    FromTimestamp,
}

impl ToString for FetchMode {
    fn to_string(&self) -> String {
        match self {
            Self::All => "All".to_string(),
            Self::Tail => "Newest".to_string(),
            Self::Head => "Oldest".to_string(),
            Self::FromTimestamp => "From date/time".to_string(),
        }
    }
}

impl FromStr for FetchMode {
    type Err = ExternalError;
    fn from_str(text: &str) -> Result<Self, ExternalError> {
        match text {
            "All" => Ok(Self::All),
            "Newest" => Ok(Self::Tail),
            "Oldest" => Ok(Self::Head),
            "From date/time" => Ok(Self::FromTimestamp),
            _ => Err(ExternalError::DisplayError(
                "fetch mode not found".to_string(),
                text.to_string(),
            )),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq)]
pub struct KrustTopicCache {
    pub connection_id: usize,
    pub topic_name: String,
    pub fetch_mode: FetchMode,
    pub fetch_value: Option<i64>,
    pub default_page_size: u16,
    pub last_updated: Option<i64>,
}

impl Display for KrustTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct KrustMessage {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<String>,
    pub value: String,
    pub timestamp: Option<i64>,
    pub headers: Vec<KrustHeader>,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct KrustHeader {
    pub key: String,
    pub value: Option<String>,
}

pub struct Repository {
    conn: rusqlite::Connection,
}
#[derive(Clone)]
pub struct MessagesRepository {
    pub topic_name: String,
    pub path: PathBuf,
    pub database_name: String,
    pub connection_id: usize,
}
#[derive(Debug, Clone)]
pub struct MessagesSearchOrder {
    pub column: String,
    pub order: String,
}

impl MessagesRepository {
    pub fn new(connection_id: usize, topic_name: &String) -> Self {
        let path = PathBuf::from(Settings::read().unwrap_or_default().cache_dir.as_str());
        let database_name = format!("topic_{}_{}", connection_id, topic_name);
        Self {
            topic_name: topic_name.clone(),
            path: path.clone(),
            database_name,
            connection_id,
        }
    }
    pub fn from_filename(filename: String) -> Self {
        static RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"topic_(?P<connection_id>\d+)_(?P<topic_name>\S+)\.db").unwrap()
        });
        let caps = RE.captures(&filename).unwrap();
        let connection_id = caps["connection_id"].parse::<usize>().unwrap();
        let topic_name = &caps["topic_name"].to_string();
        let path = PathBuf::from(Settings::read().unwrap_or_default().cache_dir.as_str());
        let database_name = format!("topic_{}_{}", connection_id, topic_name);
        Self {
            topic_name: topic_name.clone(),
            path: path.clone(),
            database_name,
            connection_id,
        }
    }
    pub fn get_connection(&self) -> Connection {
        let conn = database_connection_with_name(&self.path, &self.database_name)
            .expect("problem acquiring database connection");
        conn.execute_batch(
            "PRAGMA journal_mode = OFF;
            PRAGMA synchronous = 0;
            PRAGMA cache_size = 200000;
            PRAGMA locking_mode = EXCLUSIVE;
            PRAGMA temp_store = MEMORY;",
        )
        .unwrap();
        conn
    }
    pub fn get_init_connection(&mut self) -> Connection {
        database_connection_with_name(&self.path, &self.database_name)
            .expect("problem acquiring database connection")
    }
    pub fn init(&mut self) -> Result<(), ExternalError> {
        let result = self.get_init_connection().execute_batch(
            "CREATE TABLE IF NOT EXISTS kr_message
            (partition INTEGER, offset INTEGER, key TEXT, value TEXT, timestamp INTEGER, headers TEXT, PRIMARY KEY (partition, offset));"
        ).map_err(ExternalError::DatabaseError);
        let _ = self
            .get_init_connection()
            .execute_batch("ALTER TABLE kr_message ADD COLUMN key TEXT;")
            .ok();
        result
    }

    pub fn destroy(&mut self) -> Result<(), ExternalError> {
        destroy_database_with_name(self.path.clone(), &self.database_name)
    }

    pub fn save_message(
        &self,
        conn: &Connection,
        message: &KrustMessage,
    ) -> Result<KrustMessage, ExternalError> {
        //let conn = self.get_connection();
        let mut stmt_by_id = conn.prepare_cached(
            "INSERT INTO kr_message(partition, offset, key, value, timestamp, headers)
            VALUES (:p, :o, :k, :v, :t, :h)",
        )?;
        let headers =
            ron::ser::to_string::<Vec<KrustHeader>>(message.headers.as_ref()).unwrap_or_default();
        let maybe_message = stmt_by_id
        .execute(
            named_params! { ":p": message.partition, ":o": message.offset, ":k": message.key, ":v": message.value, ":t": message.timestamp, ":h": headers},
        )
        .map_err(ExternalError::DatabaseError);

        match maybe_message {
            Ok(_) => Ok(message.to_owned()),
            Err(e) => Err(e),
        }
    }

    pub fn count_messages(&mut self, search: Option<String>) -> Result<usize, ExternalError> {
        let conn = self.get_connection();
        let mut stmt_count = match search {
            Some(_) => {
                conn.prepare_cached("SELECT COUNT(1) FROM kr_message WHERE value LIKE :search")?
            }
            None => conn.prepare_cached("SELECT COUNT(1) FROM kr_message")?,
        };
        let params_with_search =
            named_params! { ":search": format!("%{}%", search.clone().unwrap_or_default()) };
        stmt_count
            .query_row(
                if search.is_some() {
                    params_with_search
                } else {
                    named_params![]
                },
                move |row| row.get(0),
            )
            .map_err(ExternalError::DatabaseError)
    }

    // TODO: find latest offsets/partitions

    pub fn find_offsets(&mut self) -> Result<Vec<Partition>, ExternalError> {
        let conn = self.get_connection();
        let mut stmt_by_id = conn.prepare_cached(
            "SELECT high.partition partition, offset_low, offset_high
            FROM (SELECT partition, MAX(offset) offset_high
            from kr_message
            GROUP BY partition) high
            JOIN (SELECT partition, MIN(offset) offset_low
            from kr_message
            GROUP BY partition) low ON high.partition = low.partition",
        )?;

        let row_to_model = move |row: &Row<'_>| {
            Ok(Partition {
                id: row.get(0)?,
                offset_low: Some(row.get(1)?),
                offset_high: Some(row.get(2)?),
            })
        };
        let rows = stmt_by_id
            .query_map(params![], row_to_model)
            .map_err(ExternalError::DatabaseError)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }
    fn get_pagination_from(&self, page: usize, page_size: u16) -> usize {
        (page * page_size as usize) - page_size as usize
    }
    fn get_pagination_to(&self, page: usize, page_size: u16) -> usize {
        self.get_pagination_from(page, page_size) + page_size as usize
    }
    pub fn find_messages_paged(
        &mut self,
        task: Task,
        page: usize,
        page_size: u16,
        order: Option<MessagesSearchOrder>,
        search: Option<String>,
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let conn = self.get_connection();
        let order = order
            .map(|o| format!("{} {}", o.column, o.order))
            .unwrap_or("timestamp DESC".to_string());
        let from = self.get_pagination_from(page, page_size);
        let to = self.get_pagination_to(page, page_size);
        let mut stmt_query = match search {
            Some(_) => conn.prepare_cached(
                format!(
                "SELECT partition, offset, key, value, timestamp, headers FROM (
                    SELECT ROW_NUMBER () OVER (ORDER BY {}) rownum, partition, offset, key, value, timestamp, headers
                    FROM kr_message
                    WHERE value LIKE :search)
                WHERE rownum > {} AND rownum <= {}", order, from, to).as_str(),
            )?,
            None => conn.prepare_cached(
                format!(
                "SELECT partition, offset, key, value, timestamp, headers FROM (
                    SELECT ROW_NUMBER () OVER (ORDER BY {}) rownum, partition, offset, key, value, timestamp, headers
                    FROM kr_message)
                WHERE rownum > {} AND rownum <= {}", order, from, to).as_str(),
            )?,
        };
        let string_to_headers = move |sheaders: String| {
            let headers: Result<Vec<KrustHeader>, rusqlite::Error> = ron::from_str(&sheaders)
                .map_err(|e| rusqlite::Error::InvalidColumnName(e.to_string()));
            headers
        };
        let topic_name = self.topic_name.clone();
        let row_to_model = move |row: &Row<'_>| {
            Ok(KrustMessage {
                partition: row.get(0)?,
                offset: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                timestamp: Some(row.get(4)?),
                headers: string_to_headers(row.get(5)?)?,
                topic: topic_name.clone(),
            })
        };
        let params_with_search =
            named_params! { ":search": format!("%{}%", search.clone().unwrap_or_default()) };
        let params = named_params! {};
        let rows = stmt_query
            .query_map(
                if search.is_some() {
                    params_with_search
                } else {
                    params
                },
                row_to_model,
            )
            .map_err(ExternalError::DatabaseError)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
            let progress_step = ((messages.len() as f64) * 1.0) / ((page_size as f64) * 1.0);
            TASK_MANAGER_BROKER.send(TaskManagerMsg::Progress(task.clone(), progress_step));
        }
        Ok(messages)
    }
}

impl Default for Repository {
    fn default() -> Self {
        Self::new()
    }
}

impl Repository {
    pub fn new() -> Self {
        let conn = database_connection().expect("problem acquiring database connection");
        Self { conn }
    }

    pub fn init(&mut self) -> Result<(), ExternalError> {
        self.conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS kr_connection
                (id INTEGER PRIMARY KEY,
                name TEXT UNIQUE,
                brokersList TEXT,
                securityType TEXT,
                saslMechanism TEXT,
                saslUsername TEXT,
                saslPassword TEXT);

                CREATE TABLE IF NOT EXISTS kr_topic
                (connection_id INTEGER,
                name TEXT,
                cached INTEGER,
                PRIMARY KEY (connection_id, name),
                FOREIGN KEY (connection_id) REFERENCES kr_connection(id));
                ",
            )
            .map_err(ExternalError::DatabaseError)?;
        self.conn
            .execute_batch("ALTER TABLE kr_topic ADD COLUMN favourite INTEGER DEFAULT 0;")
            .map_err(ExternalError::DatabaseError)
            .unwrap_or_else(|e| {
                warn!("kr_topic.favourite: {:?}", e);
            });
        self.conn
            .execute_batch("ALTER TABLE kr_topic DROP COLUMN favorite;")
            .map_err(ExternalError::DatabaseError)
            .unwrap_or_else(|e| {
                warn!("kr_topic.favourite: {:?}", e);
            });
        self.conn
            .execute_batch("ALTER TABLE kr_connection ADD COLUMN color TEXT DEFAULT NULL;")
            .map_err(ExternalError::DatabaseError)
            .unwrap_or_else(|e| {
                warn!("kr_topic.color: {:?}", e);
            });
        self.conn
            .execute_batch("ALTER TABLE kr_connection ADD COLUMN timeout INTEGER DEFAULT NULL;")
            .map_err(ExternalError::DatabaseError)
            .unwrap_or_else(|e| {
                warn!("kr_topic.timeout: {:?}", e);
            });
        self.conn
            .execute_batch(
                "
                PRAGMA foreign_keys=off;
                BEGIN TRANSACTION;
                ALTER TABLE kr_topic RENAME TO _kr_topic_old;
                CREATE TABLE IF NOT EXISTS kr_topic
                    (connection_id INTEGER,
                    name TEXT,
                    cached INTEGER,
                    favourite INTEGER DEFAULT 0,
                    PRIMARY KEY (connection_id, name),
                    FOREIGN KEY (connection_id) REFERENCES kr_connection(id) ON DELETE CASCADE);
                INSERT INTO kr_topic SELECT * FROM _kr_topic_old;
                COMMIT;
                PRAGMA foreign_keys=on;
                ",
            )
            .map_err(ExternalError::DatabaseError)
            .unwrap_or_else(|e| {
                warn!("kr_topic.connection_id (FK DELETE CASCADE): {:?}", e);
            });
        info!("repository::create kr_topic_cache");
        self.conn
            .execute_batch(
                "
                ROLLBACK;
                BEGIN;
                CREATE TABLE IF NOT EXISTS kr_topic_cache
                   (connection_id INTEGER,
                    topic_name TEXT,
                    fetch_mode TEXT,
                    last_updated INTEGER,
                    fetch_value INTEGER,
                    default_page_size INTEGER,
                    PRIMARY KEY (connection_id, topic_name),
                    FOREIGN KEY (connection_id, topic_name) REFERENCES kr_topic(connection_id, name) ON DELETE CASCADE);
                COMMIT;
                ",
            )
            .map_err(ExternalError::DatabaseError)
            .unwrap_or_else(|e| {
                warn!("kr_topic_cache: {:?}", e);
            });
        Ok(())
    }

    pub fn connection_by_id(&mut self, id: usize) -> Option<KrustConnection> {
        let mut stmt = self.conn.prepare_cached("
            SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword, color, timeout
            FROM kr_connection WHERE id = ?")
        .expect("Should return prepared statement");
        let rows = stmt
            .query_row(params![id], |row| {
                Ok(KrustConnection {
                    id: row.get(0).unwrap_or(None),
                    name: row.get(1).unwrap_or_default(),
                    brokers_list: row.get(2).unwrap_or_default(),
                    security_type: KrustConnectionSecurityType::from_str(
                        row.get::<usize, String>(3).unwrap_or_default().as_str(),
                    )
                    .unwrap_or_default(),
                    sasl_mechanism: row.get(4).unwrap_or(None),
                    sasl_username: row.get(5).unwrap_or(None),
                    sasl_password: row.get(6).unwrap_or(None),
                    color: row.get(7).unwrap_or(None),
                    timeout: row.get(8).unwrap_or(None),
                })
            })
            .map_err(ExternalError::DatabaseError);
        match rows {
            Ok(conn) => Some(conn),
            Err(_) => None,
        }
    }
    pub fn list_all_connections(&mut self) -> Result<Vec<KrustConnection>, ExternalError> {
        let mut stmt = self.conn.prepare_cached(
            "
        SELECT
            id
            , name
            , brokersList
            , securityType
            , saslMechanism
            , saslUsername
            , saslPassword
            , color
            , timeout
        FROM kr_connection
        ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KrustConnection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    brokers_list: row.get(2)?,
                    security_type: KrustConnectionSecurityType::from_str(
                        row.get::<usize, String>(3)?.as_str(),
                    )
                    .unwrap_or_default(),
                    sasl_mechanism: row.get(4)?,
                    sasl_username: row.get(5)?,
                    sasl_password: row.get(6)?,
                    color: row.get(7)?,
                    timeout: row.get(8)?,
                })
            })
            .map_err(ExternalError::DatabaseError)?;
        let mut connections = Vec::new();
        for row in rows {
            connections.push(row?);
        }
        Ok(connections)
    }

    pub fn save_connection(
        &mut self,
        konn: &KrustConnection,
    ) -> Result<KrustConnection, ExternalError> {
        let id = konn.id;
        let name = konn.name.clone();
        let brokers = konn.brokers_list.clone();
        let security = konn.security_type.clone();
        let sasl = konn.sasl_mechanism.clone();
        let sasl_username = konn.sasl_username.clone();
        let sasl_password = konn.sasl_password.clone();
        let color = konn.color.clone();
        let timeout = konn.timeout;
        let mut stmt_by_id = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword, color, timeout from kr_connection where id = ?1")?;
        let mut stmt_by_name = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword, color, timeout from kr_connection where name = ?1")?;
        let row_to_model = move |row: &Row<'_>| {
            Ok(KrustConnection {
                id: row.get(0)?,
                name: row.get(1)?,
                brokers_list: row.get(2)?,
                security_type: KrustConnectionSecurityType::from_str(
                    row.get::<usize, String>(3)?.as_str(),
                )
                .unwrap_or_default(),
                sasl_mechanism: row.get(4)?,
                sasl_username: row.get(5)?,
                sasl_password: row.get(6)?,
                color: row.get(7)?,
                timeout: row.get(8)?,
            })
        };
        let maybe_konn = match id {
            Some(_) => stmt_by_id
                .query_row([&id], row_to_model)
                .map_err(ExternalError::DatabaseError),
            None => stmt_by_name
                .query_row([&name], row_to_model)
                .map_err(ExternalError::DatabaseError),
        };

        match maybe_konn {
            Ok(konn_to_update) => {
                let mut up_stmt = self.conn.prepare_cached(
                    "
                    UPDATE kr_connection
                    SET name = :name
                    , brokersList = :brokers
                    , securityType = :security
                    , saslMechanism = :sasl
                    , saslUsername = :sasl_u
                    , saslPassword = :sasl_p
                    , color = :color
                    , timeout = :timeout
                    WHERE id = :id",
                )?;
                up_stmt
                    .execute(named_params! {
                        ":id": &konn_to_update.id.unwrap(),
                        ":name": &name,
                        ":brokers": &brokers,
                        ":security": security.to_string(),
                        ":sasl": &sasl,
                        ":sasl_u": &sasl_username,
                        ":sasl_p": &sasl_password,
                        ":color": &color,
                        ":timeout": &timeout,
                    })
                    .map_err(ExternalError::DatabaseError)
                    .map(|_| KrustConnection {
                        id: konn_to_update.id,
                        name,
                        brokers_list: brokers,
                        security_type: security,
                        sasl_mechanism: sasl,
                        sasl_username,
                        sasl_password,
                        color,
                        timeout,
                    })
            }
            Err(_) => {
                let mut ins_stmt = self.conn.prepare_cached("
                    INSERT INTO kr_connection (id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword, color, timeout)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    RETURNING id")?;
                ins_stmt
                    .query_row(
                        params![
                            &konn.id,
                            &konn.name,
                            &konn.brokers_list,
                            &konn.security_type.to_string(),
                            &konn.sasl_mechanism,
                            &konn.sasl_username,
                            &konn.sasl_password,
                            &konn.color,
                            &konn.timeout,
                        ],
                        |row| {
                            Ok(KrustConnection {
                                id: row.get(0)?,
                                name,
                                brokers_list: brokers,
                                security_type: security,
                                sasl_mechanism: sasl,
                                sasl_username,
                                sasl_password,
                                color,
                                timeout,
                            })
                        },
                    )
                    .map_err(ExternalError::DatabaseError)
            }
        }
        //Ok(Konnection {id: None, name: "".into()})
    }

    pub fn save_topic(
        &mut self,
        conn_id: usize,
        topic: &KrustTopic,
    ) -> Result<KrustTopic, ExternalError> {
        let name = topic.name.clone();
        let favourite = topic.favourite;
        let mut stmt_by_id = self.conn.prepare_cached(
            "INSERT INTO kr_topic(connection_id, name, favourite)
            VALUES (:cid, :topic, :favourite)
            ON CONFLICT(connection_id, name)
            DO UPDATE SET favourite=excluded.favourite",
        )?;
        let row_to_model = move |_| {
            Ok(KrustTopic {
                connection_id: Some(conn_id),
                name: topic.name.clone(),
                cached: None,
                partitions: vec![],
                total: None,
                favourite,
            })
        };

        stmt_by_id
            .execute(
                named_params! { ":cid": &conn_id, ":topic": &name.clone(), ":favourite": &favourite },
            )
            .map(row_to_model)?
            .map_err(ExternalError::DatabaseError)
    }

    pub fn save_topic_cache(
        &mut self,
        conn_id: usize,
        topic_name: String,
        cache: &KrustTopicCache,
    ) -> Result<KrustTopicCache, ExternalError> {
        info!("[save_topic_cache] saving topic cache::{:?}", cache);
        let fetch_mode = cache.fetch_mode;
        let last_updated = cache.last_updated;
        let default_page_size = cache.default_page_size;
        let fetch_value = cache.fetch_value;

        let topic = self.find_topic(conn_id, &topic_name);
        if topic.is_none() {
            info!("[save_topic_cache] creating topic for cache::{:?}", cache);
            let saved = self
                .save_topic(
                    conn_id,
                    &KrustTopic {
                        connection_id: Some(conn_id),
                        name: topic_name.clone(),
                        cached: None,
                        partitions: vec![],
                        total: None,
                        favourite: Some(false),
                    },
                )
                .expect("[save_topic_cache] should save topic for cache");
            info!("[save_topic_cache] topic created::{:?}", saved);
        };

        let mut stmt_by_id = self.conn.prepare_cached(
            "INSERT INTO kr_topic_cache(connection_id, topic_name, fetch_mode, fetch_value, last_updated, default_page_size)
            VALUES (:cid, :topic, :fetch_mode, :fetch_value, :last_updated, :default_page_size)
            ON CONFLICT(connection_id, topic_name)
            DO UPDATE SET
                            fetch_mode=excluded.fetch_mode,
                            fetch_value=excluded.fetch_value,
                            last_updated=excluded.last_updated,
                            default_page_size=excluded.default_page_size",
        )?;
        let t_name = topic_name.clone();
        let row_to_model = move |_| {
            Ok(KrustTopicCache {
                connection_id: conn_id,
                topic_name: t_name.clone(),
                fetch_mode,
                fetch_value,
                last_updated,
                default_page_size,
            })
        };

        stmt_by_id
            .execute(named_params! {
            ":cid": &conn_id,
            ":topic": topic_name.clone(),
            ":fetch_mode": &fetch_mode.to_string(),
            ":fetch_value": &fetch_value,
            ":last_updated": &last_updated,
            ":default_page_size": &default_page_size })
            .map(row_to_model)?
            .map_err(ExternalError::DatabaseError)
    }

    pub fn delete_topic(
        &mut self,
        conn_id: usize,
        topic: &KrustTopic,
    ) -> Result<usize, ExternalError> {
        let name = topic.name.clone();
        let mut stmt_by_id = self.conn.prepare_cached(
            "DELETE FROM kr_topic
            WHERE connection_id = :cid
            AND name = :topic",
        )?;

        stmt_by_id
            .execute(named_params! { ":cid": &conn_id, ":topic": &name.clone(),})
            .map_err(ExternalError::DatabaseError)
    }

    pub fn delete_topic_cache(
        &mut self,
        conn_id: usize,
        topic_name: String,
    ) -> Result<usize, ExternalError> {
        let mut stmt_by_id = self.conn.prepare_cached(
            "DELETE FROM kr_topic_cache
            WHERE connection_id = :cid
            AND topic_name = :topic",
        )?;

        stmt_by_id
            .execute(named_params! { ":cid": &conn_id, ":topic": &topic_name.clone(),})
            .map_err(ExternalError::DatabaseError)
    }

    pub fn delete_connection(&mut self, conn_id: usize) -> Result<usize, ExternalError> {
        let mut stmt_by_id = self.conn.prepare_cached(
            "DELETE FROM kr_connection
            WHERE id = :cid",
        )?;
        stmt_by_id
            .execute(named_params! { ":cid": &conn_id,})
            .map_err(ExternalError::DatabaseError)
    }

    pub fn find_topic_cache(
        &mut self,
        conn_id: usize,
        topic_name: &String,
    ) -> Option<KrustTopicCache> {
        let stmt = self.conn.prepare_cached(
            "
            SELECT
                connection_id,
                topic_name,
                fetch_mode,
                fetch_value,
                last_updated,
                default_page_size
            FROM kr_topic_cache WHERE connection_id = :cid AND topic_name = :topic",
        );
        stmt.ok()?
            .query_row(
                named_params! {":cid": &conn_id, ":topic": &topic_name },
                |row| {
                    Ok(KrustTopicCache {
                        connection_id: row.get(0)?,
                        topic_name: row.get(1)?,
                        fetch_mode: FetchMode::from_str(row.get::<usize, String>(2)?.as_str())
                            .unwrap_or_default(),
                        fetch_value: row.get(3)?,
                        last_updated: row.get(4)?,
                        default_page_size: row.get(5)?,
                    })
                },
            )
            .ok()
    }

    pub fn find_topic(&mut self, conn_id: usize, topic_name: &String) -> Option<KrustTopic> {
        let cache = self.find_topic_cache(conn_id, topic_name);
        let stmt = self.conn
        .prepare_cached("SELECT connection_id, name, favourite FROM kr_topic WHERE connection_id = :cid AND name = :topic");
        stmt.ok()?
            .query_row(
                named_params! {":cid": &conn_id, ":topic": &topic_name },
                |row| {
                    Ok(KrustTopic {
                        connection_id: row.get(0)?,
                        name: row.get(1)?,
                        cached: cache.clone(),
                        partitions: vec![],
                        total: None,
                        favourite: row.get(2)?,
                    })
                },
            )
            .ok()
    }
    pub fn find_topics_by_connection(
        &mut self,
        conn_id: usize,
    ) -> Result<Vec<KrustTopic>, ExternalError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT connection_id, name, favourite FROM kr_topic WHERE connection_id = :cid",
        )?;
        let rows = stmt
            .query_and_then(named_params! {":cid": &conn_id }, |row| {
                Ok::<KrustTopic, rusqlite::Error>(KrustTopic {
                    connection_id: row.get(0)?,
                    name: row.get(1)?,
                    cached: None,
                    partitions: vec![],
                    total: None,
                    favourite: row.get(2)?,
                })
            })
            .map_err(ExternalError::DatabaseError)?;
        let mut topics = vec![];
        for topic in rows {
            topics.push(topic?);
        }
        Ok(topics)
    }
}
