mod config;
mod health_check;
mod hosts;
mod messages;
mod queries;

use clap::Parser;
use config::read_config_file;
use hosts::Hosts;
use mysql::{Conn, OptsBuilder};

use file_guard::Lock;
use queries::query_discovery;
use std::fs::OpenOptions;

/// Readyset ProxySQL Scheduler
/// This tool is used to query ProxySQL Stats tables to find queries that are not yet cached in Readyset and then cache them.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// path to the config file
    #[arg(long)]
    config: String,
}

fn main() {
    messages::print_info("Running readyset_scheduler");
    let args = Args::parse();
    let config_file = read_config_file(&args.config).expect("Failed to read config file");
    let config = config::parse_config_file(&config_file).expect("Failed to parse config file");
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(
            config
                .clone()
                .lock_file
                .unwrap_or("/tmp/readyset_scheduler.lock".to_string()),
        ) {
        Ok(file) => file,
        Err(err) => {
            messages::print_error(
                format!(
                    "Failed to open lock file {}: {}",
                    config
                        .lock_file
                        .unwrap_or("/tmp/readyset_scheduler.lock".to_string()),
                    err
                )
                .as_str(),
            );
            std::process::exit(1);
        }
    };

    let _guard = match file_guard::try_lock(&file, Lock::Exclusive, 0, 1) {
        Ok(guard) => guard,
        Err(err) => {
            messages::print_error(format!("Failed to acquire lock: {}", err).as_str());
            std::process::exit(1);
        }
    };
    let mut proxysql_conn = Conn::new(
        OptsBuilder::new()
            .ip_or_hostname(Some(config.proxysql_host.as_str()))
            .tcp_port(config.proxysql_port)
            .user(Some(config.proxysql_user.as_str()))
            .pass(Some(config.proxysql_password.as_str()))
            .prefer_socket(false),
    )
    .unwrap();

    let running_mode = match config.operation_mode {
        Some(mode) => mode,
        None => config::OperationMode::All,
    };

    let mut hosts = Hosts::new(&mut proxysql_conn, &config);

    if running_mode == config::OperationMode::HealthCheck
        || running_mode == config::OperationMode::All
    {
        hosts.iter_mut().for_each(|host| {
            health_check::health_check(&mut proxysql_conn, &config, host);
        });
    }

    // retain only healthy hosts
    hosts.retain_online();
    if running_mode == config::OperationMode::QueryDiscovery
        || running_mode == config::OperationMode::All
    {
        query_discovery(&mut proxysql_conn, &config, &mut hosts);
    }

    messages::print_info("Finished readyset_scheduler");
}
