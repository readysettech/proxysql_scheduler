mod config;
mod health_check;
mod messages;
mod queries;
mod server;

use clap::Parser;
use config::read_config_file;
use mysql::OptsBuilder;
use mysql::{Pool, PoolConstraints, PoolOpts};

use file_guard::Lock;
use queries::query_discovery;
use server::ServerStatus;
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

    let proxysql_pool = Pool::new(
        OptsBuilder::new()
            .ip_or_hostname(Some(config.proxysql_host.as_str()))
            .tcp_port(config.proxysql_port)
            .user(Some(config.proxysql_user.as_str()))
            .pass(Some(config.proxysql_password.as_str()))
            .prefer_socket(false)
            .pool_opts(
                PoolOpts::default()
                    .with_reset_connection(false)
                    .with_constraints(PoolConstraints::new(1, 1).unwrap()),
            ),
    )
    .unwrap();
    let mut proxysql_conn = proxysql_pool.get_conn().unwrap();

    let readyset_pool = match Pool::new(
        OptsBuilder::new()
            .ip_or_hostname(Some(config.readyset_host.as_str()))
            .tcp_port(config.readyset_port)
            .user(Some(config.readyset_user.as_str()))
            .pass(Some(config.readyset_password.as_str()))
            .prefer_socket(false)
            .pool_opts(
                PoolOpts::default()
                    .with_reset_connection(false)
                    .with_constraints(PoolConstraints::new(1, 1).unwrap()),
            ),
    ) {
        Ok(conn) => conn,
        Err(e) => {
            messages::print_error(format!("Cannot connect to Readyset: {}.", e).as_str());
            let _ =
                server::change_server_status(&mut proxysql_conn, &config, ServerStatus::Shunned);
            std::process::exit(1);
        }
    };
    let mut readyset_conn = readyset_pool.get_conn().unwrap();

    let running_mode = match config.operation_mode {
        Some(mode) => mode,
        None => config::OperationMode::All,
    };

    if running_mode == config::OperationMode::HealthCheck
        || running_mode == config::OperationMode::All
    {
        health_check::health_check(&mut proxysql_conn, &config, &mut readyset_conn)
    }

    if running_mode == config::OperationMode::QueryDiscovery
        || running_mode == config::OperationMode::All
    {
        query_discovery(&mut proxysql_conn, &config, &mut readyset_conn);
    }

    messages::print_info("Finished readyset_scheduler");
}
