use chrono::{DateTime, Local};
use mysql::{prelude::Queryable, Conn, OptsBuilder};

use crate::{
    config,
    hosts::{Host, ProxyStatus},
    messages,
    queries::Query,
};

const MIRROR_QUERY_TOKEN: &str = "Mirror by readyset scheduler at";
const DESTINATION_QUERY_TOKEN: &str = "Added by readyset scheduler at";
pub struct ProxySQL {
    readyset_hostgroup: u16,
    warmup_time_s: u16,
    conn: mysql::Conn,
    hosts: Vec<Host>,
    dry_run: bool,
}

impl ProxySQL {
    /// This function is used to create a new ProxySQL struct.
    ///
    /// # Arguments
    ///
    /// * `config` - A reference to a config::Config containing the configuration for the ProxySQL connection.
    ///
    /// # Returns
    ///
    /// A new ProxySQL struct.
    pub fn new(config: &config::Config, dry_run: bool) -> Self {
        let mut conn = Conn::new(
            OptsBuilder::new()
                .ip_or_hostname(Some(config.proxysql_host.as_str()))
                .tcp_port(config.proxysql_port)
                .user(Some(config.proxysql_user.as_str()))
                .pass(Some(config.proxysql_password.as_str()))
                .prefer_socket(false),
        )
        .expect("Failed to create ProxySQL connection");

        let query = format!(
            "SELECT hostname, port, status, comment FROM mysql_servers WHERE hostgroup_id = {} AND status IN ('ONLINE', 'SHUNNED', 'OFFLINE_SOFT')",
            config.readyset_hostgroup
        );
        let results: Vec<(String, u16, String, String)> = conn.query(query).unwrap();
        let hosts = results
            .into_iter()
            .filter_map(|(hostname, port, status, comment)| {
                if comment.to_lowercase().contains("readyset") {
                    Some(Host::new(hostname, port, status, config))
                } else {
                    None
                }
            })
            .collect::<Vec<Host>>();

        ProxySQL {
            conn,
            readyset_hostgroup: config.readyset_hostgroup,
            warmup_time_s: config.warmup_time_s.unwrap_or(0),
            hosts,
            dry_run,
        }
    }

    /// This function is used to get the dry_run field.
    /// This field is used to indicate if the ProxySQL operations should be executed or not.
    ///
    /// # Returns
    ///
    /// A boolean indicating if the ProxySQL operations should be executed or not.
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// This function is used to add a query rule to ProxySQL.
    ///
    /// # Arguments
    ///
    /// * `query` - A reference to a Query containing the query to be added as a rule.
    ///
    /// # Returns
    ///
    /// A boolean indicating if the rule was added successfully.
    pub fn add_as_query_rule(&mut self, query: &Query) -> Result<bool, mysql::Error> {
        let datetime_now: DateTime<Local> = Local::now();
        let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
        if self.warmup_time_s > 0 {
            self.conn.query_drop(format!("INSERT INTO mysql_query_rules (username, mirror_hostgroup, active, digest, apply, comment) VALUES ('{}', {}, 1, '{}', 1, '{}: {}')", query.get_user(), self.readyset_hostgroup, query.get_digest(), MIRROR_QUERY_TOKEN, date_formatted)).expect("Failed to insert into mysql_query_rules");
            messages::print_note("Inserted warm-up rule");
        } else {
            self.conn.query_drop(format!("INSERT INTO mysql_query_rules (username, destination_hostgroup, active, digest, apply, comment) VALUES ('{}', {}, 1, '{}', 1, '{}: {}')", query.get_user(), self.readyset_hostgroup, query.get_digest(), DESTINATION_QUERY_TOKEN, date_formatted)).expect("Failed to insert into mysql_query_rules");
            messages::print_note("Inserted destination rule");
        }
        Ok(true)
    }

    pub fn load_query_rules(&mut self) -> Result<bool, mysql::Error> {
        self.conn
            .query_drop("LOAD MYSQL QUERY RULES TO RUNTIME")
            .expect("Failed to load query rules");
        Ok(true)
    }
    pub fn save_query_rules(&mut self) -> Result<bool, mysql::Error> {
        self.conn
            .query_drop("SAVE MYSQL QUERY RULES TO DISK")
            .expect("Failed to load query rules");
        Ok(true)
    }

    /// This function is used to check the current list of queries routed to Readyset.
    ///
    /// # Arguments
    /// * `conn` - A reference to a connection to ProxySQL.
    ///
    /// # Returns
    /// A vector of tuples containing the digest_text, digest, and schemaname of the queries that are currently routed to ReadySet.
    pub fn find_queries_routed_to_readyset(&mut self) -> Vec<String> {
        let rows: Vec<String> = self
            .conn
            .query(format!(
            "SELECT digest FROM mysql_query_rules WHERE comment LIKE '{}%' OR comment LIKE '{}%'",
            MIRROR_QUERY_TOKEN, DESTINATION_QUERY_TOKEN
        ))
            .expect("Failed to find queries routed to ReadySet");
        rows
    }

    /// This function is used to check if any mirror query rule needs to be changed to destination.
    ///
    /// # Returns
    ///
    /// A boolean indicating if any mirror query rule was changed to destination.
    pub fn adjust_mirror_rules(&mut self) -> Result<bool, mysql::Error> {
        let mut updated_rules = false;
        let datetime_now: DateTime<Local> = Local::now();
        let tz = datetime_now.format("%z").to_string();
        let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
        let rows: Vec<(u16, String)> = self.conn.query(format!("SELECT rule_id, comment FROM mysql_query_rules WHERE comment LIKE '{}: ____-__-__ __:__:__';", MIRROR_QUERY_TOKEN)).expect("Failed to select mirror rules");
        for (rule_id, comment) in rows {
            let datetime_mirror_str = comment
                .split("Mirror by readyset scheduler at:")
                .nth(1)
                .unwrap_or("")
                .trim();
            let datetime_mirror_str = format!("{} {}", datetime_mirror_str, tz);
            let datetime_mirror_rule =
                DateTime::parse_from_str(datetime_mirror_str.as_str(), "%Y-%m-%d %H:%M:%S %z")
                    .unwrap_or_else(|_| {
                        panic!("Failed to parse datetime from comment: {}", comment);
                    });
            let elapsed = datetime_now
                .signed_duration_since(datetime_mirror_rule)
                .num_seconds();
            if elapsed > self.warmup_time_s as i64 {
                let comment = format!(
                    "{}\n Added by readyset scheduler at: {}",
                    comment, date_formatted
                );
                self.conn.query_drop(format!("UPDATE mysql_query_rules SET mirror_hostgroup = NULL, destination_hostgroup = {}, comment = '{}' WHERE rule_id = {}", self.readyset_hostgroup, comment, rule_id)).expect("Failed to update rule");
                messages::print_note(
                    format!("Updated rule ID {} from warmup to destination", rule_id).as_str(),
                );
                updated_rules = true;
            }
        }
        Ok(updated_rules)
    }

    /// This function is used to check if a given host is healthy.
    /// This is done by checking if the Readyset host has an active
    /// connection and if the snapshot is completed.
    pub fn health_check(&mut self) {
        let mut status_changes = Vec::new();

        for host in self.hosts.iter_mut() {
            match host.check_readyset_is_ready() {
                Ok(ready) => match ready {
                    ProxyStatus::Online => {
                        status_changes.push((host, ProxyStatus::Online));
                    }
                    ProxyStatus::Shunned => {
                        status_changes.push((host, ProxyStatus::Shunned));
                    }
                    ProxyStatus::OfflineSoft => {
                        status_changes.push((host, ProxyStatus::OfflineSoft));
                    }
                    ProxyStatus::OfflineHard => {
                        status_changes.push((host, ProxyStatus::OfflineHard));
                    }
                },
                Err(e) => {
                    messages::print_error(format!("Cannot check Readyset status: {}.", e).as_str());
                    status_changes.push((host, ProxyStatus::Shunned));
                }
            };
        }

        for (host, status) in status_changes {
            if host.get_proxysql_status() != status {
                let where_clause = format!(
                    "WHERE hostgroup_id = {} AND hostname = '{}' AND port = {}",
                    self.readyset_hostgroup,
                    host.get_hostname(),
                    host.get_port()
                );
                messages::print_note(
                    format!(
                        "Server HG: {}, Host: {}, Port: {} is currently {} on proxysql and {} on readyset. Changing to {}",
                        self.readyset_hostgroup,
                        host.get_hostname(),
                        host.get_port(),
                        host.get_proxysql_status(),
                        host.get_readyset_status().to_string().to_uppercase(),
                        status
                    )
                    .as_str(),
                );
                host.change_proxysql_status(status);
                if self.dry_run {
                    messages::print_info("Dry run, skipping changes to ProxySQL");
                    continue;
                }
                let _ = self.conn.query_drop(format!(
                    "UPDATE mysql_servers SET status = '{}' {}",
                    host.get_proxysql_status(),
                    where_clause
                ));
                let _ = self.conn.query_drop("LOAD MYSQL SERVERS TO RUNTIME");
                let _ = self.conn.query_drop("SAVE MYSQL SERVERS TO DISK");
            }
        }
    }

    /// This function is used to get the number of online hosts.
    /// This is done by filtering the hosts vector and counting the number of hosts with status Online.
    ///
    /// # Returns
    ///
    /// A u16 containing the number of online hosts.
    pub fn number_of_online_hosts(&self) -> u16 {
        self.hosts
            .iter()
            .filter(|host| host.is_proxysql_online())
            .collect::<Vec<&Host>>()
            .len() as u16
    }

    /// This function is used to get the first online host.
    /// This is done by iterating over the hosts vector and returning the first host with status Online.
    ///
    /// # Returns
    ///
    /// An Option containing a reference to the first online host.
    pub fn get_first_online_host(&mut self) -> Option<&mut Host> {
        self.hosts.iter_mut().find(|host| host.is_proxysql_online())
    }

    /// This function is used to get all the online hosts.
    /// This is done by filtering the hosts vector and collecting the hosts with status Online.
    ///
    /// # Returns
    ///
    /// A vector containing references to the online hosts.
    pub fn get_online_hosts(&mut self) -> Vec<&mut Host> {
        self.hosts
            .iter_mut()
            .filter(|host| host.is_proxysql_online())
            .collect()
    }
}
