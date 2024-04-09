use std::str::FromStr;
use std::string::ToString;

use rusqlite::{named_params, params, Row};
use strum::EnumString;

use crate::config::{database_connection, ExternalError};

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
#[derive(Debug, Clone, Default)]
pub struct KrustMessage {
    pub id: Option<usize>,
    pub connection_id: Option<usize>,
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub value: String,
    pub timestamp: Option<i64>,
    pub headers: Vec<KrustHeader>,
}
#[derive(Debug, Clone, Default)]
pub struct KrustHeader {
    pub key: String,
    pub value: Option<String>,
}

pub struct Repository {
    conn: rusqlite::Connection,
}

impl Repository {
    pub fn new() -> Self {
        let conn = database_connection().unwrap();
        Self { conn }
    }

    pub fn init(&mut self) -> Result<(), ExternalError> {
        self.conn.execute_batch("
    CREATE TABLE IF NOT EXISTS kl_connection(id INTEGER PRIMARY KEY, name TEXT UNIQUE, brokersList TEXT, securityType TEXT, saslMechanism TEXT, saslUsername TEXT, saslPassword TEXT);
    CREATE TABLE IF NOT EXISTS kl_message (connection INTEGER, topic TEXT, partition INTEGER, offset INTEGER, value TEXT, timestamp TEXT, PRIMARY KEY (connection, topic, partition, offset));
    CREATE INDEX IF NOT EXISTS idx_connection_topic ON kl_message (connection, topic);
    CREATE INDEX IF NOT EXISTS idx_value ON kl_message (value);
    ").map_err(ExternalError::DatabaseError)
    }

    pub fn list_all_connections(&mut self) -> Result<Vec<KrustConnection>, ExternalError> {
        let mut stmt = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword from kl_connection")?;
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
        let mut stmt_by_id = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword from kl_connection where id = ?1")?;
        let mut stmt_by_name = self.conn.prepare_cached("SELECT id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword from kl_connection where name = ?1")?;
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
                let mut up_stmt = self.conn.prepare_cached("UPDATE kl_connection SET name = :name, brokersList = :brokers, securityType = :security, saslMechanism = :sasl, saslUsername = :sasl_u, saslPassword = :sasl_p WHERE id = :id")?;
                up_stmt
        .execute(named_params! { ":id": &konn_to_update.id.unwrap(), ":name": &name, ":brokers": &brokers, ":security": security.to_string(), ":sasl": &sasl, ":sasl_u": &sasl_username, ":sasl_p": &sasl_password })
        .map_err(ExternalError::DatabaseError)
        .map( |_| {KrustConnection { id: konn_to_update.id, name: name, brokers_list: brokers, security_type: security, sasl_mechanism: sasl, sasl_username: sasl_username, sasl_password: sasl_password }})
            }
            Err(_) => {
                let mut ins_stmt = self.conn.prepare_cached("INSERT INTO kl_connection (id, name, brokersList, securityType, saslMechanism, saslUsername, saslPassword) VALUES (?, ?, ?, ?, ?, ?, ?)  RETURNING id")?;
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

    pub fn delete_all_messages_for_topic(
        &mut self,
        conn_id: usize,
        topic_name: String,
    ) -> Result<usize, ExternalError> {
        let mut delete_stmt = self.conn.prepare_cached(
            "DELETE from kl_message where connection = :conn_id AND topic = :topic",
        )?;
        let result = delete_stmt
            .execute(named_params! {":conn_id": conn_id, ":topic": topic_name})
            .map_err(ExternalError::DatabaseError)?;
        Ok(result)
    }

    pub fn insert_message(&mut self, message: KrustMessage) -> Result<KrustMessage, ExternalError> {
        let KrustMessage {
            id: _,
            connection_id,
            topic,
            partition,
            offset,
            value,
            timestamp,
            headers,
        } = message;
        let mut insert_stmt = self.conn.prepare_cached("INSERT INTO kl_message (connection, topic, partition, offset, value, timestamp) VALUES (?, ?, ?, ?, ?, ?) RETURNING id")?;
        let result = insert_stmt
            .query_row(params![&message.connection_id,], |row| {
                Ok(KrustMessage {
                    id: row.get(0)?,
                    connection_id: connection_id,
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
