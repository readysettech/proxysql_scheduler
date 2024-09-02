use crate::{config::Config, messages};
use core::fmt;
use mysql::{prelude::Queryable, Conn, OptsBuilder};

#[allow(dead_code)]
/// Defines the possible status of a host
#[derive(PartialEq, Clone, Copy)]
pub enum HostStatus {
    ///backend server is fully operational
    Online,
    //backend sever is temporarily taken out of use because of either too many connection errors in a time that was too short, or the replication lag exceeded the allowed threshold
    Shunned,
    //when a server is put into OFFLINE_SOFT mode, no new connections are created toward that server, while the existing connections are kept until they are returned to the connection pool or destructed. In other words, connections are kept in use until multiplexing is enabled again, for example when a transaction is completed. This makes it possible to gracefully detach a backend as long as multiplexing is efficient
    OfflineSoft,
    //when a server is put into OFFLINE_HARD mode, no new connections are created toward that server and the existing **free **connections are ** immediately dropped**, while backend connections currently associated with a client session are dropped as soon as the client tries to use them. This is equivalent to deleting the server from a hostgroup. Internally, setting a server in OFFLINE_HARD status is equivalent to deleting the server
    OfflineHard,
}

impl fmt::Display for HostStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostStatus::Online => write!(f, "ONLINE"),
            HostStatus::Shunned => write!(f, "SHUNNED"),
            HostStatus::OfflineSoft => write!(f, "OFFLINE_SOFT"),
            HostStatus::OfflineHard => write!(f, "OFFLINE_HARD"),
        }
    }
}

impl From<String> for HostStatus {
    fn from(s: String) -> Self {
        match s.to_uppercase().as_str() {
            "ONLINE" => HostStatus::Online,
            "SHUNNED" => HostStatus::Shunned,
            "OFFLINE_SOFT" => HostStatus::OfflineSoft,
            "OFFLINE_HARD" => HostStatus::OfflineHard,
            _ => HostStatus::Online,
        }
    }
}

/// Represents a Readyset host
pub struct Host {
    hostname: String,
    port: u16,
    status: HostStatus,
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
    pub fn new(hostname: String, port: u16, status: String, config: &Config) -> Host {
        let conn = match Conn::new(
            OptsBuilder::new()
                .ip_or_hostname(Some(hostname.clone()))
                .tcp_port(port)
                .user(Some(config.readyset_user.clone()))
                .pass(Some(config.readyset_password.clone()))
                .prefer_socket(false),
        ) {
            Ok(conn) => conn,
            Err(err) => {
                eprintln!("Failed to establish connection: {}", err);
                return Host {
                    hostname,
                    port,
                    status: HostStatus::from(status),
                    conn: None,
                };
            }
        };

        Host {
            hostname,
            port,
            status: HostStatus::from(status),
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

    /// Gets the status of the host.
    ///
    /// # Returns
    ///
    /// The status of the host.
    fn get_status(&self) -> HostStatus {
        self.status
    }

    /// Checks if the host is online.
    ///
    /// # Returns
    ///
    /// true if the host is online, false otherwise.
    pub fn is_online(&self) -> bool {
        self.status == HostStatus::Online
    }

    /// Checks if the Readyset host is ready to serve traffic.
    /// This is done by querying the SHOW READYSET STATUS command.
    ///
    /// # Returns
    ///
    /// true if the host is ready, false otherwise.
    pub fn check_readyset_is_ready(&mut self) -> Result<bool, mysql::Error> {
        match &mut self.conn {
            Some(conn) => {
                let rows: Vec<(String, String)> =
                    conn.query("SHOW READYSET STATUS").unwrap_or(vec![]);
                for (field, value) in rows {
                    if field == "Snapshot Status" {
                        return Ok(value == "Completed");
                    }
                }
                Ok(false)
            }
            None => Err(mysql::Error::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
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
    pub fn cache_query(
        &mut self,
        digest_text: &String,
        digest: &String,
    ) -> Result<bool, mysql::Error> {
        match &mut self.conn {
            None => return Ok(false),
            Some(conn) => {
                conn.query_drop(format!("CREATE CACHE d_{} FROM {}", digest, digest_text))
                    .expect("Failed to create readyset cache");
            }
        }
        Ok(true)
    }

    /// Changes the status of the host in the ProxySQL mysql_servers table.
    /// The status is set to the given `status`.
    pub fn change_status(
        &mut self,
        ps_conn: &mut Conn,
        config: &Config,
        status: HostStatus,
    ) -> Result<bool, mysql::Error> {
        let where_clause = format!(
            "WHERE hostgroup_id = {} AND hostname = '{}' AND port = {}",
            config.readyset_hostgroup,
            self.get_hostname(),
            self.get_port()
        );
        if self.status != status {
            messages::print_info(
                format!(
                    "Server HG: {}, Host: {}, Port: {} is currently {}. Changing to {}",
                    config.readyset_hostgroup,
                    self.get_hostname(),
                    self.get_port(),
                    self.get_status(),
                    status
                )
                .as_str(),
            );
            self.status = status;
            ps_conn.query_drop(format!(
                "UPDATE mysql_servers SET status = '{}' {}",
                self.get_status(),
                where_clause
            ))?;
            ps_conn.query_drop("LOAD MYSQL SERVERS TO RUNTIME")?;
            ps_conn.query_drop("SAVE MYSQL SERVERS TO DISK")?;
        }

        Ok(true)
    }
}

/// Represents a list of Readyset hosts
pub struct Hosts {
    hosts: Vec<Host>,
}

impl From<Vec<Host>> for Hosts {
    fn from(hosts: Vec<Host>) -> Self {
        Hosts { hosts }
    }
}

impl Hosts {
    /// Fetches the hosts from the ProxySQL mysql_servers table.
    ///
    /// # Arguments
    ///
    /// * `proxysql_conn` - The connection to the ProxySQL instance.
    /// * `config` - The configuration object.
    ///
    /// # Returns
    ///
    /// A vector of `Host` instances.
    pub fn new<'a>(proxysql_conn: &'a mut Conn, config: &'a Config) -> Self {
        let query = format!(
            "SELECT hostname, port, status, comment FROM mysql_servers WHERE hostgroup_id = {}",
            config.readyset_hostgroup
        );
        let results: Vec<(String, u16, String, String)> = proxysql_conn.query(query).unwrap();
        results
            .into_iter()
            .filter_map(|(hostname, port, status, comment)| {
                if comment.to_lowercase().contains("readyset") {
                    Some(Host::new(hostname, port, status, config))
                } else {
                    None
                }
            })
            .collect::<Vec<Host>>()
            .into()
    }

    /// Gets a mutable iterator over the hosts.
    ///
    /// # Returns
    ///
    /// A mutable iterator over the hosts.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<Host> {
        self.hosts.iter_mut()
    }

    /// Mutate self by retaining only the hosts that are online.
    pub fn retain_online(&mut self) {
        self.hosts.retain(|host| host.is_online());
    }

    /// Checks if the hosts list is empty.
    ///
    /// # Returns
    ///
    /// true if the hosts list is empty, false otherwise.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// Gets a mutable reference for the first host in the list.
    ///
    /// # Returns
    ///
    /// A reference to the first host in the list.
    /// If the list is empty, the function returns None.
    pub fn first_mut(&mut self) -> Option<&mut Host> {
        self.hosts.first_mut()
    }
}
