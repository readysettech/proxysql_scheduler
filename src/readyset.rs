use crate::{
    config::{Config, DatabaseType},
    queries::Query,
    sql_connection::SQLConnection,
};
use anyhow::{bail, Result};
use core::fmt;

/// Defines the possible status of a Readyset instance
#[derive(PartialEq, Clone, Copy)]
pub enum ProxySQLStatus {
    /// backend server is fully operational
    Online,
    /// backend sever is temporarily taken out of use because of either too many connection errors in a time that was too short, or the replication lag exceeded the allowed threshold
    Shunned,
    /// when a server is put into OFFLINE_SOFT mode, no new connections are created toward that server, while the existing connections are kept until they are returned to the connection pool or destructed. In other words, connections are kept in use until multiplexing is enabled again, for example when a transaction is completed. This makes it possible to gracefully detach a backend as long as multiplexing is efficient
    OfflineSoft,
    /// when a server is put into OFFLINE_HARD mode, no new connections are created toward that server and the existing free connections are immediately dropped, while backend connections currently associated with a client session are dropped as soon as the client tries to use them. This is equivalent to deleting the server from a hostgroup. Internally, setting a server in OFFLINE_HARD status is equivalent to deleting the server
    OfflineHard,
}

impl fmt::Display for ProxySQLStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxySQLStatus::Online => write!(f, "ONLINE"),
            ProxySQLStatus::Shunned => write!(f, "SHUNNED"),
            ProxySQLStatus::OfflineSoft => write!(f, "OFFLINE_SOFT"),
            ProxySQLStatus::OfflineHard => write!(f, "OFFLINE_HARD"),
        }
    }
}

impl From<String> for ProxySQLStatus {
    fn from(s: String) -> Self {
        match s.to_uppercase().as_str() {
            "ONLINE" => ProxySQLStatus::Online,
            "SHUNNED" => ProxySQLStatus::Shunned,
            "OFFLINE_SOFT" => ProxySQLStatus::OfflineSoft,
            "OFFLINE_HARD" => ProxySQLStatus::OfflineHard,
            _ => ProxySQLStatus::Online,
        }
    }
}

impl From<ReadysetStatus> for ProxySQLStatus {
    fn from(status: ReadysetStatus) -> Self {
        match status {
            ReadysetStatus::Online => ProxySQLStatus::Online,
            ReadysetStatus::SnapshotInProgress => ProxySQLStatus::Shunned,
            ReadysetStatus::Maintenance => ProxySQLStatus::OfflineSoft,
            ReadysetStatus::Unknown => ProxySQLStatus::Shunned,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum ReadysetStatus {
    Online,
    SnapshotInProgress,
    Maintenance,
    Unknown,
}

impl fmt::Display for ReadysetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadysetStatus::Online => write!(f, "Online"),
            ReadysetStatus::SnapshotInProgress => write!(f, "Snapshot in progress"),
            ReadysetStatus::Maintenance => write!(f, "Maintenance mode"),
            ReadysetStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

impl From<String> for ReadysetStatus {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "online" => ReadysetStatus::Online,
            "snapshot in progress" => ReadysetStatus::SnapshotInProgress,
            "maintenance mode" => ReadysetStatus::Maintenance,
            _ => ReadysetStatus::Unknown,
        }
    }
}

/// Represents a Readyset instance
pub struct Readyset {
    database_type: DatabaseType,
    hostname: String,
    port: u16,
    proxysql_status: ProxySQLStatus,
    readyset_status: ReadysetStatus,
    conn: Option<SQLConnection>,
}

impl Readyset {
    /// Creates a new `Readyset` instance with the given hostname and port.
    /// The connection to the Readyset instance is established during the creation of the instance.
    /// If the connection fails, the `conn` field will be `None`.
    /// If the connection is successful, the `conn` field will contain the connection.
    ///
    /// # Arguments
    ///
    /// * `hostname` - The hostname of the Readyset instance.
    /// * `port` - The port number of the Readyset instance.
    /// * `proxysql_status` - The ProxySQL status of the Readyset instance.
    /// * `config` - The config for this instance of the scheduler.
    ///
    /// # Returns
    ///
    /// A new `Readyset` instance.
    pub fn new(hostname: String, port: u16, proxysql_status: String, config: &Config) -> Readyset {
        let conn = match SQLConnection::new(
            config.database_type,
            &hostname,
            port,
            &config.readyset_user,
            &config.readyset_password,
        ) {
            Ok(conn) => conn,
            Err(err) => {
                eprintln!("Failed to establish connection: {}", err);
                return Readyset {
                    database_type: config.database_type,
                    hostname,
                    port,
                    proxysql_status: ProxySQLStatus::from(proxysql_status),
                    readyset_status: ReadysetStatus::Unknown,
                    conn: None,
                };
            }
        };

        Readyset {
            database_type: config.database_type,
            hostname,
            port,
            proxysql_status: ProxySQLStatus::from(proxysql_status),
            readyset_status: ReadysetStatus::Unknown,
            conn: Some(conn),
        }
    }

    /// Gets the hostname of the Readyset instance.
    ///
    /// # Returns
    ///
    /// The hostname of the Readyset instance.
    pub fn get_hostname(&self) -> &String {
        &self.hostname
    }

    /// Gets the port of the Readyset instance.
    ///
    /// # Returns
    ///
    /// The port of the Readyset instance.
    pub fn get_port(&self) -> u16 {
        self.port
    }

    /// Gets the ProxySQL status of the Readyset instance.
    ///
    /// # Returns
    ///
    /// The ProxySQL status of the Readyset instance.
    pub fn get_proxysql_status(&self) -> ProxySQLStatus {
        self.proxysql_status
    }

    /// Changes the ProxySQL status of the Readyset instance.
    ///
    /// # Arguments
    ///
    /// * `status` - The new ProxySQL status of the Readyset instance.
    pub fn change_proxysql_status(&mut self, status: ProxySQLStatus) {
        self.proxysql_status = status;
    }

    /// Checks if the Readyset instance is online in ProxySQL.
    ///
    /// # Returns
    ///
    /// true if the Readyset instance is online, false otherwise.
    pub fn is_proxysql_online(&self) -> bool {
        self.proxysql_status == ProxySQLStatus::Online
    }

    /// Gets the Readyset status of the Readyset instance.
    ///
    /// # Returns
    ///
    /// The Readyset status of the Readyset instance.
    pub fn get_readyset_status(&self) -> ReadysetStatus {
        self.readyset_status
    }

    /// Checks if the Readyset instance is ready to serve traffic.
    /// This is done by querying the SHOW READYSET STATUS command.
    ///
    /// # Returns
    ///
    /// true if the instance is ready, false otherwise.
    pub fn check_readyset_is_ready(&mut self) -> Result<ProxySQLStatus> {
        match &mut self.conn {
            Some(conn) => {
                let result = conn.query("SHOW READYSET STATUS");
                match result {
                    Ok(rows) => {
                        let rows: Vec<(String, String)> = rows;
                        for (field, value) in rows {
                            if field == "Snapshot Status" && value == "Completed" {
                                self.readyset_status = ReadysetStatus::Online;
                                return Ok(ProxySQLStatus::Online);
                            } else if field == "Snapshot Status" && value == "In Progress" {
                                self.readyset_status = ReadysetStatus::SnapshotInProgress;
                                return Ok(ProxySQLStatus::Shunned);
                            } else if field == "Status" {
                                let status = ReadysetStatus::from(value);
                                self.readyset_status = status;
                                return Ok(status.into());
                            }
                        }
                        self.readyset_status = ReadysetStatus::Unknown;
                        Ok(ProxySQLStatus::Shunned)
                    }
                    Err(err) => bail!("Failed to execute query: {}", err),
                }
            }
            None => bail!("Connection to Readyset instance is not established"),
        }
    }

    /// Checks if the Readyset instance supports the given query.
    /// This is done by querying the EXPLAIN CREATE CACHE FROM command.
    ///
    /// # Arguments
    ///
    /// * `digest_text` - The digest text of the query.
    /// * `schema` - The schema of the query.
    ///
    /// # Returns
    ///
    /// true if the instance supports the query, false otherwise.
    pub fn check_query_support(&mut self, digest_text: &String, schema: &String) -> Result<bool> {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL Readyset query support check");
        }
        match &mut self.conn {
            Some(conn) => {
                conn.query_drop(&format!("USE {}", schema))
                    .expect("Failed to use schema");
                let row: Option<(String, String, String)> =
                    conn.query_first(&format!("EXPLAIN CREATE CACHE FROM {}", digest_text))?;
                match row {
                    Some((_, _, value)) => Ok(value == "yes" || value == "cached"),
                    None => Ok(false),
                }
            }
            None => Ok(false),
        }
    }

    /// Caches the given query on the Readyset instance.
    /// This is done by executing the CREATE CACHE FROM command.
    ///
    /// # Arguments
    ///
    /// * `query` - The query to cache.
    pub fn cache_query(&mut self, query: &Query) -> Result<()> {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL Readyset query caching");
        }
        match &mut self.conn {
            None => bail!("Connection to Readyset instance is not established"),
            Some(conn) => {
                conn.query_drop(&format!("USE {}", query.get_schema()))?;
                conn.query_drop(&format!(
                    "CREATE CACHE d_{} FROM {}",
                    query.get_digest(),
                    query.get_digest_text()
                ))?;
            }
        }
        Ok(())
    }
}
