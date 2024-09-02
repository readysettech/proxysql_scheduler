use crate::{
    config,
    hosts::{Host, HostStatus},
    messages,
};

pub fn health_check(proxysql_conn: &mut mysql::Conn, config: &config::Config, host: &mut Host) {
    match host.check_readyset_is_ready() {
        Ok(ready) => {
            if ready {
                let _ = host.change_status(proxysql_conn, config, HostStatus::Online);
            } else {
                messages::print_info("Readyset is still running Snapshot.");
                let _ = host.change_status(proxysql_conn, config, HostStatus::Shunned);
            }
        }
        Err(e) => {
            messages::print_error(format!("Cannot check Readyset status: {}.", e).as_str());
            let _ = host.change_status(proxysql_conn, config, HostStatus::Shunned);
        }
    };
}
