use std::time::Duration;

use anyhow::Result;
use mysql::{
    prelude::{FromRow, Queryable},
    Conn, OptsBuilder,
};

use crate::config::DatabaseType;

const TIMEOUT: Duration = Duration::from_secs(5);

#[allow(dead_code)]
pub enum SQLConnection {
    MySQL(Conn),
    PostgreSQL,
}

impl SQLConnection {
    pub fn new(
        database_type: DatabaseType,
        hostname: &str,
        port: u16,
        user: &str,
        pass: &str,
    ) -> Result<Self> {
        Ok(match database_type {
            DatabaseType::MySQL => Self::MySQL(Conn::new(
                OptsBuilder::new()
                    .ip_or_hostname(Some(hostname))
                    .tcp_port(port)
                    .user(Some(user))
                    .pass(Some(pass))
                    .prefer_socket(false)
                    .read_timeout(Some(TIMEOUT))
                    .write_timeout(Some(TIMEOUT))
                    .tcp_connect_timeout(Some(TIMEOUT)),
            )?),
            DatabaseType::PostgreSQL => todo!("PostgreSQL connections"),
        })
    }

    pub fn query<T: FromRow>(&mut self, query: &str) -> Result<Vec<T>> {
        Ok(match self {
            SQLConnection::MySQL(conn) => conn.query(query)?,
            SQLConnection::PostgreSQL => todo!("PostgreSQL query"),
        })
    }

    pub fn query_first<T: FromRow>(&mut self, query: &str) -> Result<Option<T>> {
        Ok(match self {
            SQLConnection::MySQL(conn) => conn.query_first(query)?,
            SQLConnection::PostgreSQL => todo!("PostgreSQL query_first"),
        })
    }

    pub fn query_drop(&mut self, query: &str) -> Result<()> {
        match self {
            SQLConnection::MySQL(conn) => conn.query_drop(query)?,
            SQLConnection::PostgreSQL => todo!("PostgreSQL query_drop"),
        }
        Ok(())
    }
}
