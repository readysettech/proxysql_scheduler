use crate::{
    config, messages,
    server::{self, ServerStatus},
};

pub fn health_check(
    proxysql_conn: &mut mysql::PooledConn,
    config: &config::Config,
    readyset_conn: &mut mysql::PooledConn,
) {
    match server::check_readyset_is_ready(readyset_conn) {
        Ok(ready) => {
            if ready {
                let _ = server::change_server_status(proxysql_conn, config, ServerStatus::Online);
            } else {
                messages::print_info("Readyset is still running Snapshot.");
                let _ = server::change_server_status(proxysql_conn, config, ServerStatus::Shunned);
                std::process::exit(0);
            }
        }
        Err(e) => {
            messages::print_error(format!("Cannot check Readyset status: {}.", e).as_str());
            let _ = server::change_server_status(proxysql_conn, config, ServerStatus::Shunned);
            std::process::exit(1);
        }
    };
}
