use crate::{config::Config, queries::Query};
use core::fmt;
use mysql::{prelude::Queryable, Conn, OptsBuilder};
use std::time::Duration;

#[allow(dead_code)]
/// Defines the possible status of a host
#[derive(PartialEq, Clone, Copy)]
pub enum ProxyStatus {
    /// backend server is fully operational
    Online,
    /// backend sever is temporarily taken out of use because of either too many connection errors in a time that was too short, or the replication lag exceeded the allowed threshold
    Shunned,
    /// when a server is put into OFFLINE_SOFT mode, no new connections are created toward that server, while the existing connections are kept until they are returned to the connection pool or destructed. In other words, connections are kept in use until multiplexing is enabled again, for example when a transaction is completed. This makes it possible to gracefully detach a backend as long as multiplexing is efficient
    OfflineSoft,
    /// when a server is put into OFFLINE_HARD mode, no new connections are created toward that server and the existing free connections are immediately dropped, while backend connections currently associated with a client session are dropped as soon as the client tries to use them. This is equivalent to deleting the server from a hostgroup. Internally, setting a server in OFFLINE_HARD status is equivalent to deleting the server
    OfflineHard,
}

impl fmt::Display for ProxyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyStatus::Online => write!(f, "ONLINE"),
            ProxyStatus::Shunned => write!(f, "SHUNNED"),
            ProxyStatus::OfflineSoft => write!(f, "OFFLINE_SOFT"),
            ProxyStatus::OfflineHard => write!(f, "OFFLINE_HARD"),
        }
    }
}

impl From<String> for ProxyStatus {
    fn from(s: String) -> Self {
        match s.to_uppercase().as_str() {
            "ONLINE" => ProxyStatus::Online,
            "SHUNNED" => ProxyStatus::Shunned,
            "OFFLINE_SOFT" => ProxyStatus::OfflineSoft,
            "OFFLINE_HARD" => ProxyStatus::OfflineHard,
            _ => ProxyStatus::Online,
        }
    }
}

impl From<ReadysetStatus> for ProxyStatus {
    fn from(status: ReadysetStatus) -> Self {
        match status {
            ReadysetStatus::Online => ProxyStatus::Online,
            ReadysetStatus::SnapshotInProgress => ProxyStatus::Shunned,
            ReadysetStatus::Maintenance => ProxyStatus::OfflineSoft,
            ReadysetStatus::Unknown => ProxyStatus::Shunned,
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

/// Represents a Readyset host
pub struct Host {
    hostname: String,
    port: u16,
    proxysql_status: ProxyStatus,
    readyset_status: ReadysetStatus,
    conn: Option<Conn>,
}

impl Host {
    /// Creates a new `Host` instance with the given hostname and port.
    /// The connection to the host is established during the creation of the instance.
    /// If the connection fails, the `conn` field will be `None`.
    /// If the connection is successful, the `conn` field will contain the connection.
    ///
    /// # Arguments
    ///
    /// * `hostname` - The hostname of the host.
    /// * `port` - The port number of the host.
    ///
    /// # Returns
    ///
    /// A new `Host` instance.
    pub fn new(hostname: String, port: u16, proxysql_status: String, config: &Config) -> Host {
        let conn = match Conn::new(
            OptsBuilder::new()
                .ip_or_hostname(Some(hostname.clone()))
                .tcp_port(port)
                .user(Some(config.readyset_user.clone()))
                .pass(Some(config.readyset_password.clone()))
                .prefer_socket(false)
                .read_timeout(Some(Duration::from_secs(5)))
                .write_timeout(Some(Duration::from_secs(5)))
                .tcp_connect_timeout(Some(Duration::from_secs(5))),
        ) {
            Ok(conn) => conn,
            Err(err) => {
                eprintln!("Failed to establish connection: {}", err);
                return Host {
                    hostname,
                    port,
                    proxysql_status: ProxyStatus::from(proxysql_status),
                    readyset_status: ReadysetStatus::Unknown,
                    conn: None,
                };
            }
        };

        Host {
            hostname,
            port,
            proxysql_status: ProxyStatus::from(proxysql_status),
            readyset_status: ReadysetStatus::Unknown,
            conn: Some(conn),
        }
    }

    /// Gets the hostname of the host.
    ///
    /// # Returns
    ///
    /// The hostname of the host.
    pub fn get_hostname(&self) -> &String {
        &self.hostname
    }

    /// Gets the port of the host.
    ///
    /// # Returns
    ///
    /// The port of the host.
    pub fn get_port(&self) -> u16 {
        self.port
    }

    /// Gets the proxysql status of the host.
    ///
    /// # Returns
    ///
    /// The status of the host.
    pub fn get_proxysql_status(&self) -> ProxyStatus {
        self.proxysql_status
    }

    /// Changes the proxysql status of the host.
    ///
    /// # Arguments
    ///
    /// * `status` - The new status of the host.
    pub fn change_proxysql_status(&mut self, status: ProxyStatus) {
        self.proxysql_status = status;
    }

    /// Checks if the host is online in proxysql.
    ///
    /// # Returns
    ///
    /// true if the host is online, false otherwise.
    pub fn is_proxysql_online(&self) -> bool {
        self.proxysql_status == ProxyStatus::Online
    }

    /// Gets the readyset status of the host.
    ///
    /// # Returns
    ///
    /// The status of the host.
    pub fn get_readyset_status(&self) -> ReadysetStatus {
        self.readyset_status
    }

    /// Checks if the Readyset host is ready to serve traffic.
    /// This is done by querying the SHOW READYSET STATUS command.
    ///
    /// # Returns
    ///
    /// true if the host is ready, false otherwise.
    pub fn check_readyset_is_ready(&mut self) -> Result<ProxyStatus, mysql::Error> {
        match &mut self.conn {
            Some(conn) => {
                let result = conn.query("SHOW READYSET STATUS");
                match result {
                    Ok(rows) => {
                        let rows: Vec<(String, String)> = rows;
                        for (field, value) in rows {
                            if field == "Snapshot Status" && value == "Completed" {
                                self.readyset_status = ReadysetStatus::Online;
                                return Ok(ProxyStatus::Online);
                            } else if field == "Snapshot Status" && value == "In Progress" {
                                self.readyset_status = ReadysetStatus::SnapshotInProgress;
                                return Ok(ProxyStatus::Shunned);
                            } else if field == "Status" {
                                let status = ReadysetStatus::from(value);
                                self.readyset_status = status;
                                return Ok(status.into());
                            }
                        }
                        self.readyset_status = ReadysetStatus::Unknown;
                        Ok(ProxyStatus::Shunned)
                    }
                    Err(err) => Err(mysql::Error::IoError(std::io::Error::other(format!(
                        "Failed to execute query: {}",
                        err
                    )))),
                }
            }
            None => Err(mysql::Error::IoError(std::io::Error::other(
                "Connection to Readyset host is not established",
            ))),
        }
    }

    /// Checks if the host supports the given query.
    /// This is done by querying the EXPLAIN CREATE CACHE FROM command.
    ///
    /// # Arguments
    ///
    /// * `digest_text` - The digest text of the query.
    /// * `schema` - The schema of the query.
    ///
    /// # Returns
    ///
    /// true if the host supports the query, false otherwise.
    pub fn check_query_support(
        &mut self,
        digest_text: &String,
        schema: &String,
    ) -> Result<bool, mysql::Error> {
        match &mut self.conn {
            Some(conn) => {
                conn.query_drop(format!("USE {}", schema))
                    .expect("Failed to use schema");
                let row: Option<(String, String, String)> =
                    conn.query_first(format!("EXPLAIN CREATE CACHE FROM {}", digest_text))?;
                match row {
                    Some((_, _, value)) => Ok(value == "yes" || value == "cached"),
                    None => Ok(false),
                }
            }
            None => Ok(false),
        }
    }

    /// Caches the given query on the host.
    /// This is done by executing the CREATE CACHE FROM command.
    ///
    /// # Arguments
    ///
    /// * `digest_text` - The digest text of the query.
    ///
    /// # Returns
    ///
    /// true if the query was cached successfully, false otherwise.
    pub fn cache_query(&mut self, query: &Query) -> Result<bool, mysql::Error> {
        match &mut self.conn {
            None => {
                return Err(mysql::Error::IoError(std::io::Error::other(
                    "Connection to Readyset host is not established",
                )))
            }
            Some(conn) => {
                conn.query_drop(format!("USE {}", query.get_schema()))?;
                conn.query_drop(format!(
                    "CREATE CACHE d_{} FROM {}",
                    query.get_digest(),
                    query.get_digest_text()
                ))?;
            }
        }
        Ok(true)
    }
}
