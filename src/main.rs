mod config;
mod messages;
mod queries;
mod server;

use clap::Parser;
use config::read_config_file;
use mysql::OptsBuilder;
use mysql::{Pool, PoolConstraints, PoolOpts};

use file_guard::Lock;
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

    match server::check_readyset_is_ready(&mut readyset_conn) {
        Ok(ready) => {
            if ready {
                let _ =
                    server::change_server_status(&mut proxysql_conn, &config, ServerStatus::Online);
            } else {
                messages::print_info("Readyset is still running Snapshot.");
                let _ = server::change_server_status(
                    &mut proxysql_conn,
                    &config,
                    ServerStatus::Shunned,
                );
                std::process::exit(0);
            }
        }
        Err(e) => {
            messages::print_error(format!("Cannot check Readyset status: {}.", e).as_str());
            let _ =
                server::change_server_status(&mut proxysql_conn, &config, ServerStatus::Shunned);
            std::process::exit(1);
        }
    };

    let mut queries_added_or_change =
        queries::adjust_mirror_rules(&mut proxysql_conn, &config).unwrap();

    let rows: Vec<(String, String, String)> =
        queries::find_queries_to_cache(&mut proxysql_conn, &config);

    for (digest_text, digest, schema) in rows {
        let digest_text = queries::replace_placeholders(&digest_text);
        messages::print_info(format!("Going to test query support for {}", digest_text).as_str());
        let supported =
            queries::check_readyset_query_support(&mut readyset_conn, &digest_text, &schema);
        match supported {
            Ok(true) => {
                messages::print_info(
                    "Query is supported, adding it to proxysql and readyset"
                        .to_string()
                        .as_str(),
                );
                queries_added_or_change = true;
                queries::cache_query(&mut readyset_conn, &digest_text)
                    .expect("Failed to create readyset cache");
                queries::add_query_rule(&mut proxysql_conn, &digest, &config)
                    .expect("Failed to add query rule");
            }
            Ok(false) => {
                messages::print_info("Query is not supported");
            }
            Err(err) => {
                messages::print_warning(format!("Failed to check query support: {}", err).as_str());
            }
        }
    }
    if queries_added_or_change {
        queries::load_query_rules(&mut proxysql_conn).expect("Failed to load query rules");
        queries::save_query_rules(&mut proxysql_conn).expect("Failed to save query rules");
    }
    messages::print_info("Finished readyset_scheduler");
}
