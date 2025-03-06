mod config;
mod messages;
mod proxysql;
mod queries;
mod readyset;
mod sql_connection;

use clap::Parser;
use config::{read_config_file, OperationMode};
use file_guard::Lock;
use proxysql::ProxySQL;
use std::fs::OpenOptions;

/// Readyset ProxySQL Scheduler
/// This tool is used to query ProxySQL stats tables to find queries that are not yet cached in Readyset and then cache them.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// path to the config file
    #[arg(long)]
    config: String,
    /// Dry run mode
    #[arg(long)]
    dry_run: bool,
}

fn main() {
    let args = Args::parse();
    let config_file = read_config_file(&args.config).expect("Failed to read config file");
    let config = config::parse_config_file(&config_file).expect("Failed to parse config file");
    messages::set_log_verbosity(config.log_verbosity);
    messages::print_info("Running readyset_scheduler");
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&config.lock_file)
    {
        Ok(file) => file,
        Err(err) => {
            messages::print_error(
                format!("Failed to open lock file {}: {}", config.lock_file, err).as_str(),
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

    let mut proxysql = ProxySQL::new(&config, args.dry_run);

    if config.operation_mode == OperationMode::HealthCheck
        || config.operation_mode == OperationMode::All
    {
        proxysql.health_check();
    }

    if config.operation_mode == OperationMode::QueryDiscovery
        || config.operation_mode == OperationMode::All
    {
        let mut query_discovery = queries::QueryDiscovery::new(&config);
        query_discovery.run(&mut proxysql);
    }

    messages::print_info("Finished readyset_scheduler");
}
