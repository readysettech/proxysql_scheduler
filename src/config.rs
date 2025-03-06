use serde::Deserialize;
use std::{
    fmt::{Display, Formatter},
    fs::File,
    io::Read,
};

use crate::messages::MessageType;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    #[default]
    MySQL,
    PostgreSQL,
}

#[derive(Deserialize, Clone, Copy, PartialEq, PartialOrd, Default, Debug)]
pub enum OperationMode {
    HealthCheck,
    QueryDiscovery,
    #[default]
    All,
}

impl From<String> for OperationMode {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "health_check" => OperationMode::HealthCheck,
            "query_discovery" => OperationMode::QueryDiscovery,
            "all" => OperationMode::All,
            _ => OperationMode::All,
        }
    }
}

impl Display for OperationMode {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            OperationMode::HealthCheck => write!(f, "health_check"),
            OperationMode::QueryDiscovery => write!(f, "query_discovery"),
            OperationMode::All => write!(f, "all"),
        }
    }
}

#[derive(Deserialize, Clone, Copy, PartialEq, PartialOrd, Default, Debug)]
pub enum QueryDiscoveryMode {
    #[default]
    CountStar,
    SumTime,
    SumRowsSent,
    MeanTime,
    ExecutionTimeDistance,
    QueryThroughput,
    WorstBestCase,
    WorstWorstCase,
    DistanceMeanMax,
    External,
}

fn default_lock_file() -> String {
    "/tmp/readyset_scheduler.lock".to_string()
}

fn default_number_of_queries() -> u16 {
    10
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub database_type: DatabaseType,
    pub proxysql_user: String,
    pub proxysql_password: String,
    pub proxysql_host: String,
    pub proxysql_port: u16,
    pub readyset_user: String,
    pub readyset_password: String,
    pub source_hostgroup: u16,
    pub readyset_hostgroup: u16,
    #[serde(default)]
    pub warmup_time_s: u16,
    #[serde(default = "default_lock_file")]
    pub lock_file: String,
    #[serde(default)]
    pub operation_mode: OperationMode,
    #[serde(default = "default_number_of_queries")]
    pub number_of_queries: u16,
    #[serde(default)]
    pub query_discovery_mode: QueryDiscoveryMode,
    #[serde(default)]
    pub query_discovery_min_execution: u64,
    #[serde(default)]
    pub query_discovery_min_row_sent: u64,
    #[serde(default)]
    pub log_verbosity: MessageType,
}

pub fn read_config_file(path: &str) -> Result<String, std::io::Error> {
    let mut file =
        File::open(path).unwrap_or_else(|_| panic!("Failed to open config file at path {}", path));
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

pub fn parse_config_file(contents: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(contents)
}
