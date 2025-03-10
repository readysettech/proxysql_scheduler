use std::time::Duration;

use anyhow::Result;
use mysql::{
    prelude::{FromRow, Queryable},
    Conn, OptsBuilder,
};
use native_tls::TlsConnector;
use postgres::{Client, Config, SimpleQueryMessage, SimpleQueryRow};
use postgres_native_tls::MakeTlsConnector;

use crate::config::DatabaseType;

const TIMEOUT: Duration = Duration::from_secs(5);

pub enum SQLConnection {
    MySQL(Conn),
    PostgreSQL(Client),
}

pub enum SQLRow<T: FromRow> {
    MySQL(T),
    PostgreSQL(SimpleQueryRow),
}

pub enum SQLRows<T: FromRow> {
    MySQL(Vec<T>),
    PostgreSQL(Vec<SimpleQueryRow>),
}

impl SQLConnection {
    pub fn new(
        database_type: DatabaseType,
        hostname: &str,
        port: u16,
        user: &str,
        pass: &str,
        database: Option<&str>,
    ) -> Result<Self> {
        Ok(match database_type {
            DatabaseType::MySQL => Self::MySQL(Conn::new(
                OptsBuilder::new()
                    .ip_or_hostname(Some(hostname))
                    .tcp_port(port)
                    .user(Some(user))
                    .pass(Some(pass))
                    .db_name(database)
                    .prefer_socket(false)
                    .read_timeout(Some(TIMEOUT))
                    .write_timeout(Some(TIMEOUT))
                    .tcp_connect_timeout(Some(TIMEOUT)),
            )?),
            DatabaseType::PostgreSQL => {
                let mut config = Config::new();
                config.host(hostname);
                config.port(port);
                config.user(user);
                config.password(pass);
                if let Some(database) = database {
                    config.dbname(database);
                }
                config.connect_timeout(TIMEOUT);
                config.tcp_user_timeout(TIMEOUT);
                Self::PostgreSQL(
                    config.connect(MakeTlsConnector::new(
                        TlsConnector::builder()
                            .danger_accept_invalid_certs(true)
                            .build()?,
                    ))?,
                )
            }
        })
    }

    pub fn query<T: FromRow>(&mut self, query: &str) -> Result<SQLRows<T>> {
        Ok(match self {
            SQLConnection::MySQL(conn) => SQLRows::MySQL(conn.query(query)?),
            SQLConnection::PostgreSQL(conn) => SQLRows::PostgreSQL(
                conn.simple_query(query)?
                    .into_iter()
                    .filter_map(|msg| {
                        if let SimpleQueryMessage::Row(row) = msg {
                            Some(row)
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
        })
    }

    pub fn query_first<T: FromRow>(&mut self, query: &str) -> Result<Option<SQLRow<T>>> {
        Ok(match self {
            SQLConnection::MySQL(conn) => conn.query_first(query)?.map(|row| SQLRow::MySQL(row)),
            SQLConnection::PostgreSQL(conn) => {
                conn.simple_query(query)?.into_iter().find_map(|msg| {
                    if let SimpleQueryMessage::Row(row) = msg {
                        Some(SQLRow::PostgreSQL(row))
                    } else {
                        None
                    }
                })
            }
        })
    }

    pub fn query_drop(&mut self, query: &str) -> Result<()> {
        match self {
            SQLConnection::MySQL(conn) => conn.query_drop(query)?,
            SQLConnection::PostgreSQL(conn) => {
                conn.simple_query(query)?;
            }
        }
        Ok(())
    }
}
