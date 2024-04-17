use std::fmt;
use std::path::PathBuf;
use std::string::ToString;
use std::{fmt::Display, str::FromStr};

use rusqlite::{named_params, params, Row};
use serde::{Deserialize, Serialize};
use strum::EnumString;

use crate::config::{
    database_connection, database_connection_with_name, destroy_database_with_name, ExternalError,
};

use super::settings::Settings;

#[allow(non_camel_case_types)]
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
    pub cached: Option<i64>,
    pub partitions: Vec<Partition>,
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
pub struct MessagesRepository {
    conn: rusqlite::Connection,
    topic_name: String,
    path: PathBuf,
    database_name: String,
}

impl MessagesRepository {
    pub fn new(connection_id: usize, topic_name: &String) -> Self {
        let path = PathBuf::from(Settings::read().unwrap().cache_dir);
        let database_name = format!("topic_{}_{}", connection_id, topic_name);
        let conn = database_connection_with_name(&path, &database_name)
            .expect("problem acquiring database connection");
        conn.execute_batch(
            "PRAGMA journal_mode = OFF;
            PRAGMA synchronous = 0;
            PRAGMA cache_size = 200000;
            PRAGMA locking_mode = EXCLUSIVE;
            PRAGMA temp_store = MEMORY;",
        )
        .unwrap();
        Self {
            conn: conn,
            topic_name: topic_name.clone(),
            path: path.clone(),
            database_name: database_name,
        }
    }

    pub fn init(&mut self) -> Result<(), ExternalError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kr_message (partition INTEGER, offset INTEGER, value TEXT, timestamp INTEGER, headers TEXT, PRIMARY KEY (partition, offset));"
        ).map_err(ExternalError::DatabaseError)
    }

    pub fn destroy(&mut self) -> Result<(), ExternalError> {
        destroy_database_with_name(self.path.clone(), &self.database_name)
    }

    pub fn save_message(&mut self, message: &KrustMessage) -> Result<KrustMessage, ExternalError> {
        let mut stmt_by_id = self.conn.prepare_cached(
            "INSERT INTO kr_message(partition, offset, value, timestamp, headers)
                    VALUES (:p, :o, :v, :t, :h)",
        )?;
        let headers =
            ron::ser::to_string::<Vec<KrustHeader>>(message.headers.as_ref()).unwrap_or_default();
        let maybe_message = stmt_by_id
            .execute(
                named_params! { ":p": message.partition, ":o": message.offset, ":v": message.value, ":t": message.timestamp, ":h": headers},
            )
            .map_err(ExternalError::DatabaseError);

        match maybe_message {
            Ok(_) => Ok(message.to_owned()),
            Err(e) => Err(e),
        }
    }

    pub fn count_messages(&mut self) -> Result<usize, ExternalError> {
        let mut stmt_by_id = self
            .conn
            .prepare_cached("SELECT COUNT(1) FROM kr_message")?;
        let maybe_count = stmt_by_id
            .query_row(params![], move |row| Ok(row.get(0)?))
            .map_err(ExternalError::DatabaseError);

        maybe_count
    }

    // TODO: find latest offsets/partitions

    pub fn find_offsets(&mut self) -> Result<Vec<Partition>, ExternalError>{
        let mut stmt_by_id = self.conn.prepare_cached(
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

    pub fn find_messages(&mut self, page_size: u16) -> Result<Vec<KrustMessage>, ExternalError> {
        let mut stmt_by_id = self.conn.prepare_cached(
            "SELECT partition, offset, value, timestamp, headers
                   FROM kr_message
               ORDER BY offset, partition
               LIMIT :ps",
        )?;
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
                value: row.get(2)?,
                timestamp: Some(row.get(3)?),
                headers: string_to_headers(row.get(4)?)?,
                topic: topic_name.clone(),
            })
        };
        let rows = stmt_by_id
            .query_map(named_params! {":ps": page_size}, row_to_model)
            .map_err(ExternalError::DatabaseError)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    pub fn find_next_messages(
        &mut self,
        page_size: u16,
        last: (usize, usize),
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let (offset, partition) = last;
        let mut stmt_by_id = self.conn.prepare_cached(
            "SELECT partition, offset, value, timestamp, headers
                   FROM kr_message
                  WHERE (offset, partition) > (:o, :p)
               ORDER BY offset, partition
                  LIMIT :ps",
        )?;
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
                value: row.get(2)?,
                timestamp: Some(row.get(3)?),
                headers: string_to_headers(row.get(4)?)?,
                topic: topic_name.clone(),
            })
        };
        let rows = stmt_by_id
            .query_map(
                named_params! {":ps": page_size, ":o": offset, ":p": partition},
                row_to_model,
            )
            .map_err(ExternalError::DatabaseError)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }
    pub fn find_prev_messages(
        &mut self,
        page_size: u16,
        first: (usize, usize),
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let (offset, partition) = first;
        let mut stmt_by_id = self.conn.prepare_cached(
            "SELECT partition, offset, value, timestamp, headers FROM (
                 SELECT partition, offset, value, timestamp, headers
                   FROM kr_message
                  WHERE (offset, partition) < (:o, :p)
               ORDER BY offset DESC, partition DESC
                  LIMIT :ps
            ) ORDER BY offset ASC, partition ASC",
        )?;
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
                value: row.get(2)?,
                timestamp: Some(row.get(3)?),
                headers: string_to_headers(row.get(4)?)?,
                topic: topic_name.clone(),
            })
        };
        let rows = stmt_by_id
            .query_map(
                named_params! {":ps": page_size, ":o": offset, ":p": partition},
                row_to_model,
            )
            .map_err(ExternalError::DatabaseError)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }
}

impl Repository {
    pub fn new() -> Self {
        let conn = database_connection().expect("problem acquiring database connection");
        Self { conn }
    }

    pub fn init(&mut self) -> Result<(), ExternalError> {
        self.conn.execute_batch("
    CREATE TABLE IF NOT EXISTS kr_connection(id INTEGER PRIMARY KEY, name TEXT UNIQUE, brokersList TEXT, securityType TEXT, saslMechanism TEXT, saslUsername TEXT, saslPassword TEXT);
    CREATE TABLE IF NOT EXISTS kr_topic(connection_id INTEGER, name TEXT, cached INTEGER, PRIMARY KEY (connection_id, name), FOREIGN KEY (connection_id) REFERENCES kr_connection(id));
    ").map_err(ExternalError::DatabaseError)
    }

    pub fn list_all_connections(&mut self) -> Result<Vec<KrustConnection>, ExternalError> {
        let mut stmt = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword from kr_connection")?;
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
        let id = konn.id.clone();
        let name = konn.name.clone();
        let brokers = konn.brokers_list.clone();
        let security = konn.security_type.clone();
        let sasl = konn.sasl_mechanism.clone();
        let sasl_username = konn.sasl_username.clone();
        let sasl_password = konn.sasl_password.clone();
        let mut stmt_by_id = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword from kr_connection where id = ?1")?;
        let mut stmt_by_name = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword from kr_connection where name = ?1")?;
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
                let mut up_stmt = self.conn.prepare_cached("UPDATE kr_connection SET name = :name, brokersList = :brokers, securityType = :security, saslMechanism = :sasl, saslUsername = :sasl_u, saslPassword = :sasl_p WHERE id = :id")?;
                up_stmt
        .execute(named_params! { ":id": &konn_to_update.id.unwrap(), ":name": &name, ":brokers": &brokers, ":security": security.to_string(), ":sasl": &sasl, ":sasl_u": &sasl_username, ":sasl_p": &sasl_password })
        .map_err(ExternalError::DatabaseError)
        .map( |_| {KrustConnection { id: konn_to_update.id, name: name, brokers_list: brokers, security_type: security, sasl_mechanism: sasl, sasl_username: sasl_username, sasl_password: sasl_password }})
            }
            Err(_) => {
                let mut ins_stmt = self.conn.prepare_cached("INSERT INTO kr_connection (id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword) VALUES (?, ?, ?, ?, ?, ?, ?)  RETURNING id")?;
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
                        ],
                        |row| {
                            Ok(KrustConnection {
                                id: row.get(0)?,
                                name: name,
                                brokers_list: brokers,
                                security_type: security,
                                sasl_mechanism: sasl,
                                sasl_username: sasl_username,
                                sasl_password: sasl_password,
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
        let cached = topic.cached.clone();
        let mut stmt_by_id = self.conn.prepare_cached(
            "INSERT INTO kr_topic(connection_id, name, cached)
                    VALUES (:cid, :topic, :cached)
                    ON CONFLICT(connection_id, name)
                        DO UPDATE SET cached=excluded.cached",
        )?;
        let row_to_model = move |_| {
            Ok(KrustTopic {
                connection_id: Some(conn_id),
                name: topic.name.clone(),
                cached: cached,
                partitions: vec![],
            })
        };
        let maybe_topic = stmt_by_id
            .execute(
                named_params! { ":cid": &conn_id, ":topic": &name.clone(), ":cached": &cached },
            )
            .map(row_to_model)?
            .map_err(ExternalError::DatabaseError);

        maybe_topic
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
        let maybe_topic = stmt_by_id
            .execute(named_params! { ":cid": &conn_id, ":topic": &name.clone(),})
            .map_err(ExternalError::DatabaseError);

        maybe_topic
    }

    pub fn find_topic(&mut self, conn_id: usize, topic_name: &String) -> Option<KrustTopic> {
        let stmt = self.conn
            .prepare_cached("SELECT connection_id, name, cached FROM kr_topic WHERE connection_id = :cid AND name = :topic");
        stmt.ok()?
            .query_row(
                named_params! {":cid": &conn_id, ":topic": &topic_name },
                |row| {
                    Ok(KrustTopic {
                        connection_id: row.get(0)?,
                        name: row.get(1)?,
                        cached: row.get(2)?,
                        partitions: vec![],
                    })
                },
            )
            .ok()
    }

    pub fn delete_all_messages_for_topic(
        &mut self,
        conn_id: usize,
        topic_name: String,
    ) -> Result<usize, ExternalError> {
        let mut delete_stmt = self.conn.prepare_cached(
            "DELETE from kr_message where connection = :conn_id AND topic = :topic",
        )?;
        let result = delete_stmt
            .execute(named_params! {":conn_id": conn_id, ":topic": topic_name})
            .map_err(ExternalError::DatabaseError)?;
        Ok(result)
    }

    pub fn insert_message(&mut self, message: KrustMessage) -> Result<KrustMessage, ExternalError> {
        let KrustMessage {
            topic,
            partition,
            offset,
            value,
            timestamp,
            headers,
        } = message;
        let mut insert_stmt = self.conn.prepare_cached("INSERT INTO kr_message (connection, topic, partition, offset, value, timestamp) VALUES (?, ?, ?, ?, ?, ?) RETURNING id")?;
        let result = insert_stmt
            .query_row(params![], |_row| {
                Ok(KrustMessage {
                    topic: topic,
                    partition: partition,
                    offset: offset,
                    value: value,
                    timestamp: timestamp,
                    headers: headers,
                })
            })
            .map_err(ExternalError::DatabaseError)?;
        Ok(result)
    }
}
