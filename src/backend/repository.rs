use std::fmt;
use std::path::PathBuf;
use std::string::ToString;
use std::{fmt::Display, str::FromStr};

use rusqlite::{named_params, params, Connection, Row};
use serde::{Deserialize, Serialize};
use strum::EnumString;
use tracing::warn;

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
    pub total: Option<usize>,
    pub favourite: Option<bool>,
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
pub struct MessagesRepository {
    topic_name: String,
    path: PathBuf,
    database_name: String,
}

impl MessagesRepository {
    pub fn new(connection_id: usize, topic_name: &String) -> Self {
        let path = PathBuf::from(Settings::read().unwrap_or_default().cache_dir.as_str());
        let database_name = format!("topic_{}_{}", connection_id, topic_name);
        Self {
            topic_name: topic_name.clone(),
            path: path.clone(),
            database_name,
        }
    }

    pub fn get_connection(&mut self) -> Connection {
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
        let conn = database_connection_with_name(&self.path, &self.database_name)
            .expect("problem acquiring database connection");
        conn
    }
    pub fn init(&mut self) -> Result<(), ExternalError> {
        let result = self.get_init_connection().execute_batch(
            "CREATE TABLE IF NOT EXISTS kr_message (partition INTEGER, offset INTEGER, key TEXT, value TEXT, timestamp INTEGER, headers TEXT, PRIMARY KEY (partition, offset));"
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
        &mut self,
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

    pub fn find_messages(
        &mut self,
        page_size: u16,
        search: Option<String>,
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let conn = self.get_connection();
        let mut stmt_query = match search {
            Some(_) => conn.prepare_cached(
                "SELECT partition, offset, key, value, timestamp, headers
                FROM kr_message
                WHERE value LIKE :search
                ORDER BY offset, partition
                LIMIT :ps",
            )?,
            None => conn.prepare_cached(
                "SELECT partition, offset, key, value, timestamp, headers
                FROM kr_message
                ORDER BY offset, partition
                LIMIT :ps",
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
        let params_with_search = named_params! { ":search": format!("%{}%", search.clone().unwrap_or_default()), ":ps": page_size };
        let params = named_params! { ":ps": page_size, };
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
        }
        Ok(messages)
    }

    pub fn find_next_messages(
        &mut self,
        page_size: u16,
        last: (usize, usize),
        search: Option<String>,
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let (offset, partition) = last;
        let conn = self.get_connection();
        let mut stmt_query = match search {
            Some(_) => conn.prepare_cached(
                "SELECT partition, offset, key, value, timestamp, headers
                FROM kr_message
                WHERE (offset, partition) > (:o, :p)
                AND value LIKE :search
                ORDER BY offset, partition
                LIMIT :ps",
            )?,
            None => conn.prepare_cached(
                "SELECT partition, offset, key, value, timestamp, headers
                FROM kr_message
                WHERE (offset, partition) > (:o, :p)
                ORDER BY offset, partition
                LIMIT :ps",
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
        let params_with_search = named_params! {":ps": page_size, ":o": offset, ":p": partition, ":search": format!("%{}%", search.clone().unwrap_or_default())};
        let params = named_params! {":ps": page_size, ":o": offset, ":p": partition,};
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
        }
        Ok(messages)
    }
    pub fn find_prev_messages(
        &mut self,
        page_size: u16,
        first: (usize, usize),
        search: Option<String>,
    ) -> Result<Vec<KrustMessage>, ExternalError> {
        let (offset, partition) = first;
        let conn = self.get_connection();
        let mut stmt_query = match search {
            Some(_) => conn.prepare_cached(
                "SELECT partition, offset, key, value, timestamp, headers FROM (
                    SELECT partition, offset, key, value, timestamp, headers
                    FROM kr_message
                    WHERE (offset, partition) < (:o, :p)
                    AND value LIKE :search
                    ORDER BY offset DESC, partition DESC
                    LIMIT :ps
                ) ORDER BY offset ASC, partition ASC",
            )?,
            None => conn.prepare_cached(
                "SELECT partition, offset, key, value, timestamp, headers FROM (
                    SELECT partition, offset, key, value, timestamp, headers
                    FROM kr_message
                    WHERE (offset, partition) < (:o, :p)
                    ORDER BY offset DESC, partition DESC
                    LIMIT :ps
                ) ORDER BY offset ASC, partition ASC",
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
        let params_with_search = named_params! {":ps": page_size, ":o": offset, ":p": partition, ":search": format!("%{}%", search.clone().unwrap_or_default()) };
        let params = named_params! {":ps": page_size, ":o": offset, ":p": partition };
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
        let _result = self.conn.execute_batch("
        CREATE TABLE IF NOT EXISTS kr_connection(id INTEGER PRIMARY KEY, name TEXT UNIQUE, brokersList TEXT, securityType TEXT, saslMechanism TEXT, saslUsername TEXT, saslPassword TEXT);
        CREATE TABLE IF NOT EXISTS kr_topic(connection_id INTEGER, name TEXT, cached INTEGER, PRIMARY KEY (connection_id, name), FOREIGN KEY (connection_id) REFERENCES kr_connection(id));
        ").map_err(ExternalError::DatabaseError)?;
        let _result_add_topic_fav = self
            .conn
            .execute_batch(
                "
        ALTER TABLE kr_topic ADD COLUMN favourite INTEGER DEFAULT 0;
        ",
            )
            .map_err(ExternalError::DatabaseError).unwrap_or_else(|e| {
                warn!("kr_topic.favourite: {:?}", e);
            });
        Ok(())
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
        let id = konn.id;
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
                .map( |_| {KrustConnection { id: konn_to_update.id, name, brokers_list: brokers, security_type: security, sasl_mechanism: sasl, sasl_username, sasl_password }})
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
                                name,
                                brokers_list: brokers,
                                security_type: security,
                                sasl_mechanism: sasl,
                                sasl_username,
                                sasl_password,
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
        let cached = topic.cached;
        let favourite = topic.favourite;
        let mut stmt_by_id = self.conn.prepare_cached(
            "INSERT INTO kr_topic(connection_id, name, cached, favourite)
            VALUES (:cid, :topic, :cached, :favourite)
            ON CONFLICT(connection_id, name)
            DO UPDATE SET cached=excluded.cached, favourite=excluded.favourite",
        )?;
        let row_to_model = move |_| {
            Ok(KrustTopic {
                connection_id: Some(conn_id),
                name: topic.name.clone(),
                cached,
                partitions: vec![],
                total: None,
                favourite,
            })
        };

        stmt_by_id
            .execute(
                named_params! { ":cid": &conn_id, ":topic": &name.clone(), ":cached": &cached, ":favourite": &favourite },
            )
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

    pub fn find_topic(&mut self, conn_id: usize, topic_name: &String) -> Option<KrustTopic> {
        let stmt = self.conn
        .prepare_cached("SELECT connection_id, name, cached, favourite FROM kr_topic WHERE connection_id = :cid AND name = :topic");
        stmt.ok()?
            .query_row(
                named_params! {":cid": &conn_id, ":topic": &topic_name },
                |row| {
                    Ok(KrustTopic {
                        connection_id: row.get(0)?,
                        name: row.get(1)?,
                        cached: row.get(2)?,
                        partitions: vec![],
                        total: None,
                        favourite: row.get(3)?,
                    })
                },
            )
            .ok()
    }
    pub fn find_topics_by_connection(&mut self, conn_id: usize) -> Result<Vec<KrustTopic>, ExternalError> {
        let mut stmt = self.conn
        .prepare_cached("SELECT connection_id, name, cached, favourite FROM kr_topic WHERE connection_id = :cid")?;
        let rows = stmt
            .query_and_then(
                named_params! {":cid": &conn_id },
                |row| {
                    Ok::<KrustTopic, rusqlite::Error>(KrustTopic {
                        connection_id: row.get(0)?,
                        name: row.get(1)?,
                        cached: row.get(2)?,
                        partitions: vec![],
                        total: None,
                        favourite: row.get(3)?,
                    })
                },
            ).map_err(ExternalError::DatabaseError)?;
        let mut topics = vec![];
        for topic in rows {
            topics.push(topic?);
        }
        Ok(topics)
    }
}
