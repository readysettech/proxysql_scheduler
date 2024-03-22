use std::{fs::File, io::Read};


#[derive(serde::Deserialize, Clone)]
pub struct Config {
    pub proxysql_user: String,
    pub proxysql_password: String,
    pub proxysql_host: String,
    pub proxysql_port: u16,
    pub readyset_user: String,
    pub readyset_password: String,
    pub readyset_host: String,
    pub readyset_port: u16,
    pub source_hostgroup: u16,
    pub readyset_hostgroup: u16,
    pub warmup_time: Option<u16>,
    pub lock_file: Option<String>,
}

pub fn read_config_file(path: &str) -> Result<String, std::io::Error> {
    let mut file = File::open(path).expect(format!("Failed to open config file at path {}", path).as_str());
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

pub fn parse_config_file(contents: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(contents)
}