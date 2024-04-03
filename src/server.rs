use core::fmt;

use crate::{config::Config, messages};
use mysql::{prelude::Queryable, PooledConn};

#[allow(dead_code)]
pub enum ServerStatus {
    //backend server is fully operational
    Online,
    //backend sever is temporarily taken out of use because of either too many connection errors in a time that was too short, or the replication lag exceeded the allowed threshold
    Shunned,
    //when a server is put into OFFLINE_SOFT mode, no new connections are created toward that server, while the existing connections are kept until they are returned to the connection pool or destructed. In other words, connections are kept in use until multiplexing is enabled again, for example when a transaction is completed. This makes it possible to gracefully detach a backend as long as multiplexing is efficient
    OfflineSoft,
    //when a server is put into OFFLINE_HARD mode, no new connections are created toward that server and the existing **free **connections are ** immediately dropped**, while backend connections currently associated with a client session are dropped as soon as the client tries to use them. This is equivalent to deleting the server from a hostgroup. Internally, setting a server in OFFLINE_HARD status is equivalent to deleting the server
    OfflineHard,
}

impl fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerStatus::Online => write!(f, "ONLINE"),
            ServerStatus::Shunned => write!(f, "SHUNNED"),
            ServerStatus::OfflineSoft => write!(f, "OFFLINE_SOFT"),
            ServerStatus::OfflineHard => write!(f, "OFFLINE_HARD"),
        }
    }
}

pub fn check_readyset_is_ready(rs_conn: &mut PooledConn) -> Result<bool, mysql::Error> {
    let rows: Vec<(String, String)> = rs_conn.query("SHOW READYSET STATUS").unwrap_or(vec![]);
    for (field, value) in rows {
        if field == "Snapshot Status" {
            return Ok(value == "Completed");
        }
    }
    Ok(false)
}

pub fn change_server_status(
    ps_conn: &mut PooledConn,
    config: &Config,
    server_status: ServerStatus,
) -> Result<bool, mysql::Error> {
    let where_clause = format!(
        "WHERE hostgroup_id = {} AND hostname = '{}' AND port = {}",
        config.readyset_hostgroup, config.readyset_host, config.readyset_port
    );
    let select_query = format!("SELECT status FROM runtime_mysql_servers {}", where_clause);
    let status: Option<String> = ps_conn.query_first(select_query)?;
    if status.as_ref().unwrap() != &server_status.to_string() {
        messages::print_info(
            format!(
                "Server HG: {}, Host: {}, Port: {} is currently {}. Changing to {}",
                config.readyset_hostgroup,
                config.readyset_host,
                config.readyset_port,
                status.unwrap(),
                server_status.to_string()
            )
            .as_str(),
        );
        ps_conn.query_drop(format!(
            "UPDATE mysql_servers SET status = '{}' {}",
            server_status.to_string(),
            where_clause
        ))?;
        ps_conn.query_drop("LOAD MYSQL SERVERS TO RUNTIME")?;
        ps_conn.query_drop("SAVE MYSQL SERVERS TO DISK")?;
    }

    Ok(true)
}
