use chrono::{DateTime, Local};

use crate::{
    config::{Config, DatabaseType},
    messages,
    queries::Query,
    readyset::{ProxySQLStatus, Readyset},
    sql_connection::SQLConnection,
};

const MIRROR_QUERY_TOKEN: &str = "Mirror by readyset scheduler at";
const DESTINATION_QUERY_TOKEN: &str = "Added by readyset scheduler at";

pub struct ProxySQL {
    database_type: DatabaseType,
    readyset_hostgroup: u16,
    warmup_time_s: u16,
    conn: SQLConnection,
    readysets: Vec<Readyset>,
    dry_run: bool,
}

impl ProxySQL {
    /// This function is used to create a new ProxySQL struct.
    ///
    /// # Arguments
    ///
    /// * `config` - The config for this instance of the scheduler.
    /// * `dry_run` - Whether or not ProxySQL operations should be executed.
    ///
    /// # Returns
    ///
    /// A new ProxySQL struct.
    pub fn new(config: &Config, dry_run: bool) -> Self {
        let mut conn = SQLConnection::new(
            config.database_type,
            &config.proxysql_host,
            config.proxysql_port,
            &config.proxysql_user,
            &config.proxysql_password,
        )
        .expect("Failed to create ProxySQL connection");
        if config.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL server management");
        }
        let query = &format!(
            "SELECT hostname, port, status, comment FROM mysql_servers WHERE hostgroup_id = {} AND status IN ('ONLINE', 'SHUNNED', 'OFFLINE_SOFT')",
            config.readyset_hostgroup
        );
        let results: Vec<(String, u16, String, String)> = conn.query(query).unwrap();
        let readysets = results
            .into_iter()
            .filter_map(|(hostname, port, status, comment)| {
                if comment.to_lowercase().contains("readyset") {
                    Some(Readyset::new(hostname, port, status, config))
                } else {
                    None
                }
            })
            .collect::<Vec<Readyset>>();

        ProxySQL {
            database_type: config.database_type,
            conn,
            readyset_hostgroup: config.readyset_hostgroup,
            warmup_time_s: config.warmup_time_s,
            readysets,
            dry_run,
        }
    }

    /// Indicates if ProxySQL operations should be executed or not.
    ///
    /// # Returns
    ///
    /// A boolean indicating if ProxySQL operations should be executed or not.
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// This function is used to add a query rule to ProxySQL.
    ///
    /// # Arguments
    ///
    /// * `query` - A reference to a Query containing the query to be added as a rule.
    pub fn add_as_query_rule(&mut self, query: &Query) {
        let datetime_now: DateTime<Local> = Local::now();
        let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL query rule management");
        }
        if self.warmup_time_s > 0 {
            self.conn.query_drop(&format!("INSERT INTO mysql_query_rules (username, mirror_hostgroup, active, digest, apply, comment) VALUES ('{}', {}, 1, '{}', 1, '{}: {}')", query.get_user(), self.readyset_hostgroup, query.get_digest(), MIRROR_QUERY_TOKEN, date_formatted)).expect("Failed to insert into mysql_query_rules");
            messages::print_note("Inserted warm-up rule");
        } else {
            self.conn.query_drop(&format!("INSERT INTO mysql_query_rules (username, destination_hostgroup, active, digest, apply, comment) VALUES ('{}', {}, 1, '{}', 1, '{}: {}')", query.get_user(), self.readyset_hostgroup, query.get_digest(), DESTINATION_QUERY_TOKEN, date_formatted)).expect("Failed to insert into mysql_query_rules");
            messages::print_note("Inserted destination rule");
        }
    }

    pub fn load_query_rules(&mut self) {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL query rule loading");
        }
        self.conn
            .query_drop("LOAD MYSQL QUERY RULES TO RUNTIME")
            .expect("Failed to load query rules");
    }

    pub fn save_query_rules(&mut self) {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL query rule saving");
        }
        self.conn
            .query_drop("SAVE MYSQL QUERY RULES TO DISK")
            .expect("Failed to save query rules");
    }

    pub fn update_servers(
        &mut self,
        hostgroup: u16,
        hostname: &str,
        port: u16,
        new_status: ProxySQLStatus,
    ) {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL server updating");
        }
        self.conn
            .query_drop(&format!(
                "UPDATE mysql_servers SET status = '{new_status}'
                 WHERE hostgroup_id = {hostgroup} AND hostname = '{hostname}' AND port = {port}"
            ))
            .expect("Failed to update servers");
    }

    pub fn load_servers(&mut self) {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL server loading");
        }
        self.conn
            .query_drop("LOAD MYSQL SERVERS TO RUNTIME")
            .expect("Failed to load servers");
    }

    pub fn save_servers(&mut self) {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL server saving");
        }
        self.conn
            .query_drop("SAVE MYSQL SERVERS TO DISK")
            .expect("Failed to save servers");
    }

    /// This function is used to check the current list of queries routed to Readyset.
    ///
    /// # Returns
    /// A vector of tuples containing the digest_text, digest, and schemaname of the queries that are currently routed to Readyset.
    pub fn find_queries_routed_to_readyset(&mut self) -> Vec<String> {
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL query rule detection");
        }
        let rows: Vec<String> = self
            .conn
            .query(&format!(
                "SELECT digest FROM mysql_query_rules WHERE comment LIKE '{MIRROR_QUERY_TOKEN}%' OR comment LIKE '{DESTINATION_QUERY_TOKEN}%'"
            ))
            .expect("Failed to find queries routed to Readyset");
        rows
    }

    /// This function is used to check if any mirror query rule needs to be changed to destination.
    ///
    /// # Returns
    ///
    /// A boolean indicating if any mirror query rule was changed to destination.
    pub fn adjust_mirror_rules(&mut self) -> bool {
        let mut updated_rules = false;
        let datetime_now: DateTime<Local> = Local::now();
        let tz = datetime_now.format("%z").to_string();
        let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
        if self.database_type == DatabaseType::PostgreSQL {
            todo!("PostgreSQL ProxySQL query rule updating");
        }
        let rows: Vec<(u16, String)> = self.conn.query(&format!("SELECT rule_id, comment FROM mysql_query_rules WHERE comment LIKE '{MIRROR_QUERY_TOKEN}: ____-__-__ __:__:__';")).expect("Failed to select mirror rules");
        for (rule_id, comment) in rows {
            let datetime_mirror_str = comment
                .split(&format!("{MIRROR_QUERY_TOKEN}:"))
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
                let comment = format!("{comment}\n {DESTINATION_QUERY_TOKEN}: {date_formatted}");
                self.conn.query_drop(&format!("UPDATE mysql_query_rules SET mirror_hostgroup = NULL, destination_hostgroup = {}, comment = '{}' WHERE rule_id = {}", self.readyset_hostgroup, comment, rule_id)).expect("Failed to update rule");
                messages::print_note(
                    format!("Updated rule ID {} from warmup to destination", rule_id).as_str(),
                );
                updated_rules = true;
            }
        }
        updated_rules
    }

    /// This function is used to check if a given Readyset instance is healthy.
    /// This is done by checking if the Readyset instance has an active
    /// connection and if the snapshot is completed.
    pub fn health_check(&mut self) {
        let mut status_changes = Vec::new();

        for (readyset_idx, readyset) in self.readysets.iter_mut().enumerate() {
            match readyset.check_readyset_is_ready() {
                Ok(ready) => match ready {
                    ProxySQLStatus::Online => {
                        status_changes.push((readyset_idx, ProxySQLStatus::Online));
                    }
                    ProxySQLStatus::Shunned => {
                        status_changes.push((readyset_idx, ProxySQLStatus::Shunned));
                    }
                    ProxySQLStatus::OfflineSoft => {
                        status_changes.push((readyset_idx, ProxySQLStatus::OfflineSoft));
                    }
                    ProxySQLStatus::OfflineHard => {
                        status_changes.push((readyset_idx, ProxySQLStatus::OfflineHard));
                    }
                },
                Err(e) => {
                    messages::print_error(format!("Cannot check Readyset status: {}.", e).as_str());
                    status_changes.push((readyset_idx, ProxySQLStatus::Shunned));
                }
            };
        }

        for (readyset_idx, status) in status_changes {
            let readyset = self.readysets.get(readyset_idx).unwrap();
            if readyset.get_proxysql_status() != status {
                messages::print_note(
                    format!(
                        "Server HG: {}, Host: {}, Port: {} is currently {} on ProxySQL and {} on Readyset. Changing to {}",
                        self.readyset_hostgroup,
                        readyset.get_hostname(),
                        readyset.get_port(),
                        readyset.get_proxysql_status(),
                        readyset.get_readyset_status().to_string().to_uppercase(),
                        status
                    )
                    .as_str(),
                );
            }
            let readyset = self.readysets.get_mut(readyset_idx).unwrap();
            readyset.change_proxysql_status(status);
            if self.dry_run {
                messages::print_info("Dry run, skipping changes to ProxySQL");
                continue;
            }
            let readyset = self.readysets.get(readyset_idx).unwrap();
            self.update_servers(
                self.readyset_hostgroup,
                readyset.get_hostname().clone().as_str(),
                readyset.get_port(),
                readyset.get_proxysql_status(),
            );
            self.load_servers();
            self.save_servers();
        }
    }

    /// This function is used to get the number of online Readyset instances.
    /// This is done by filtering the readysets vector and counting the number of Readyset instances with status Online.
    ///
    /// # Returns
    ///
    /// A u16 containing the number of online Readyset instances.
    pub fn number_of_online_readyset_instances(&self) -> u16 {
        self.readysets
            .iter()
            .filter(|readyset| readyset.is_proxysql_online())
            .collect::<Vec<&Readyset>>()
            .len() as u16
    }

    /// This function is used to get the first online Readyset instance.
    /// This is done by iterating over the readysets vector and returning the first instance with status Online.
    ///
    /// # Returns
    ///
    /// An Option containing a reference to the first online Readyset instance.
    pub fn get_first_online_readyset(&mut self) -> Option<&mut Readyset> {
        self.readysets
            .iter_mut()
            .find(|readyset| readyset.is_proxysql_online())
    }

    /// This function is used to get all the online Readyset instances.
    /// This is done by filtering the readysets vector and collecting the instance with status Online.
    ///
    /// # Returns
    ///
    /// A vector containing references to the online Readyset instances.
    pub fn get_online_readyset_instances(&mut self) -> Vec<&mut Readyset> {
        self.readysets
            .iter_mut()
            .filter(|readyset| readyset.is_proxysql_online())
            .collect()
    }

    /// Returns a reference to the current connection to ProxySQL.
    pub fn get_connection(&mut self) -> &mut SQLConnection {
        &mut self.conn
    }
}
