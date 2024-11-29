use crate::{
    config::{Config, QueryDiscoveryMode},
    messages,
    proxysql::ProxySQL,
};
use mysql::{prelude::Queryable, Conn};

pub struct Query {
    digest_text: String,
    digest: String,
    schema: String,
    user: String,
}

impl Query {
    /// This function is used to create a new Query struct.
    ///
    /// # Arguments
    ///
    /// * `digest_text` - A string containing the digest text of the query.
    /// * `digest` - A string containing the digest of the query.
    /// * `schema` - A string containing the schema name of the query.
    /// * `user` - A string containing the user that executed the query.
    ///
    /// # Returns
    ///
    /// A new Query struct.
    fn new(digest_text: String, digest: String, schema: String, user: String) -> Self {
        Query {
            digest_text,
            digest,
            schema,
            user,
        }
    }

    /// This function is used to get the digest text of the query.
    ///
    /// # Returns
    /// A string containing the digest text of the query.
    pub fn get_digest_text(&self) -> &String {
        &self.digest_text
    }

    /// This function is used to get the digest of the query.
    ///
    /// # Returns
    ///
    /// A string containing the digest of the query.
    pub fn get_digest(&self) -> &String {
        &self.digest
    }

    /// This function is used to get the schema name of the query.
    ///
    /// # Returns
    ///
    /// A string containing the schema name of the query.
    pub fn get_schema(&self) -> &String {
        &self.schema
    }

    /// This function is used to get the user that executed the query.
    ///
    /// # Returns
    ///
    /// A string containing the user that executed the query.
    pub fn get_user(&self) -> &String {
        &self.user
    }
}

pub struct QueryDiscovery {
    query_discovery_mode: QueryDiscoveryMode,
    query_discovery_min_execution: u64,
    query_discovery_min_rows_sent: u64,
    source_hostgroup: u16,
    readyset_user: String,
    number_of_queries: u16,
    offset: u16,
}

/// Query Discovery is a feature responsible for discovering queries that are hurting the database performance.
/// The queries are discovered by analyzing the stats_mysql_query_digest table and finding queries that are not cached in ReadySet and are not in the mysql_query_rules table.
/// The query discover is also responsible for promoting the queries from mirror(warmup) to destination.
impl QueryDiscovery {
    /// This function is used to create a new QueryDiscovery struct.
    ///
    /// # Arguments
    ///
    /// * `query_discovery_mode` - A QueryDiscoveryMode containing the mode to use for query discovery.
    /// * `config` - A Config containing the configuration for the query discovery.
    /// * `offset` - A u16 containing the offset to use for query discovery.
    ///
    /// # Returns
    ///
    /// A new QueryDiscovery struct.
    pub fn new(config: Config) -> Self {
        QueryDiscovery {
            query_discovery_mode: config
                .query_discovery_mode
                .unwrap_or(QueryDiscoveryMode::CountStar),
            query_discovery_min_execution: config.query_discovery_min_execution.unwrap_or(0),
            query_discovery_min_rows_sent: config.query_discovery_min_row_sent.unwrap_or(0),
            source_hostgroup: config.source_hostgroup,
            readyset_user: config.readyset_user.clone(),
            number_of_queries: config.number_of_queries,
            offset: 0,
        }
    }

    /// This function is used to generate the query responsible for finding queries that are not cached in ReadySet and are not in the mysql_query_rules table.
    /// Queries have to return 3 fields: digest_text, digest, and schema name.
    ///
    /// # Arguments
    ///
    /// * `query_discovery_mode` - A QueryDiscoveryMode containing the mode to use for query discovery.
    ///
    /// # Returns
    ///
    /// A string containing the query responsible for finding queries that are not cached in ReadySet and are not in the mysql_query_rules table.
    fn query_builder(&self) -> String {
        let order_by = match self.query_discovery_mode {
            QueryDiscoveryMode::SumRowsSent => "s.sum_rows_sent".to_string(),
            QueryDiscoveryMode::SumTime => "s.sum_time".to_string(),
            QueryDiscoveryMode::MeanTime => "(s.sum_time / s.count_star)".to_string(),
            QueryDiscoveryMode::CountStar => "s.count_star".to_string(),
            QueryDiscoveryMode::ExecutionTimeDistance => "(s.max_time - s.min_time)".to_string(),
            QueryDiscoveryMode::QueryThroughput => "(s.count_star / s.sum_time)".to_string(),
            QueryDiscoveryMode::WorstBestCase => "s.min_time".to_string(),
            QueryDiscoveryMode::WorstWorstCase => "s.max_time".to_string(),
            QueryDiscoveryMode::DistanceMeanMax => {
                "(s.max_time - (s.sum_time / s.count_star))".to_string()
            }
            QueryDiscoveryMode::External => unreachable!("External mode is caught earlier"),
        };

        format!(
            "SELECT s.digest_text, s.digest, s.schemaname
    FROM stats_mysql_query_digest s 
    LEFT JOIN mysql_query_rules q 
    USING(digest) 
    WHERE s.hostgroup = {}
    AND s.username = '{}'
    AND s.schemaname NOT IN ('sys', 'information_schema', 'performance_schema', 'mysql')
    AND s.digest_text LIKE 'SELECT%FROM%'
    AND digest_text NOT LIKE '%?=?%'
    AND s.count_star > {}
    AND s.sum_rows_sent > {}
    AND q.rule_id IS NULL
    ORDER BY {} DESC
    LIMIT {} OFFSET {}",
            self.source_hostgroup,
            self.readyset_user,
            self.query_discovery_min_execution,
            self.query_discovery_min_rows_sent,
            order_by,
            self.number_of_queries,
            self.offset
        )
    }

    pub fn run(&mut self, proxysql: &mut ProxySQL, conn: &mut Conn) {
        if proxysql.number_of_online_hosts() == 0 {
            return;
        }

        let mut queries_added_or_change = proxysql.adjust_mirror_rules().unwrap();

        let mut current_queries_digest: Vec<String> = proxysql.find_queries_routed_to_readyset();

        let mut more_queries = true;
        while more_queries && current_queries_digest.len() < self.number_of_queries as usize {
            let queries_to_cache = self.find_queries_to_cache(conn);
            more_queries = !queries_to_cache.is_empty();
            for query in queries_to_cache[0..queries_to_cache.len()].iter() {
                if current_queries_digest.len() > self.number_of_queries as usize {
                    break;
                }
                let digest_text = self.replace_placeholders(query.get_digest_text());
                messages::print_note(
                    format!("Going to test query support for {}", digest_text).as_str(),
                );
                let supported = proxysql
                    .get_first_online_host()
                    .unwrap()
                    .check_query_support(&digest_text, query.get_schema()); // Safe to unwrap because we checked if hosts is empty
                match supported {
                    Ok(true) => {
                        messages::print_note(
                            "Query is supported, adding it to proxysql and readyset"
                                .to_string()
                                .as_str(),
                        );
                        queries_added_or_change = true;
                        if !proxysql.dry_run() {
                            proxysql.get_online_hosts().iter_mut().for_each(|host| {
                                host.cache_query(query).expect(
                                    format!(
                                        "Failed to create readyset cache on host {}:{}",
                                        host.get_hostname(),
                                        host.get_port()
                                    )
                                    .as_str(),
                                );
                            });
                            proxysql
                                .add_as_query_rule(query)
                                .expect("Failed to add query rule");
                        } else {
                            messages::print_info("Dry run, not adding query");
                        }
                        current_queries_digest.push(query.get_digest().to_string());
                    }
                    Ok(false) => {
                        messages::print_note("Query is not supported");
                    }
                    Err(err) => {
                        messages::print_warning(
                            format!("Failed to check query support: {}", err).as_str(),
                        );
                    }
                }
            }
            self.offset += queries_to_cache.len() as u16;
        }
        if queries_added_or_change {
            proxysql
                .load_query_rules()
                .expect("Failed to load query rules");
            proxysql
                .save_query_rules()
                .expect("Failed to save query rules");
        }
    }

    /// This function is used to find queries that are not cached in ReadySet and are not in the mysql_query_rules table.
    ///
    /// # Arguments
    /// * `conn` - A reference to a connection to ProxySQL.
    /// * `config` - A reference to the configuration struct.
    ///
    /// # Returns
    /// A vector of tuples containing the digest_text, digest, and schema name of the queries that are not cached in ReadySet and are not in the mysql_query_rules table.
    fn find_queries_to_cache(&self, con: &mut Conn) -> Vec<Query> {
        match self.query_discovery_mode {
            QueryDiscoveryMode::External => {
                todo!("External mode is not implemented yet");
            }
            _ => {
                let query = self.query_builder();
                let rows: Vec<(String, String, String)> =
                    con.query(query).expect("Failed to find queries to cache");
                rows.iter()
                    .map(|(digest_text, digest, schema)| {
                        Query::new(
                            self.replace_placeholders(digest_text),
                            digest.to_string(),
                            schema.to_string(),
                            self.readyset_user.clone(),
                        )
                    })
                    .collect()
            }
        }
    }

    fn replace_placeholders(&self, query: &str) -> String {
        // date placeholder
        // multiple placeholders
        query.replace("?,?,?,...", "?,?,?").replace("?-?-?", "?")
    }
}
