use chrono::{DateTime, Local};
use mysql::{prelude::Queryable, PooledConn};

use crate::{
    config::{self, Config},
    messages,
};

pub fn find_queries_to_cache(
    conn: &mut PooledConn,
    config: &Config,
) -> Vec<(String, String, String)> {
    let rows: Vec<(String, String, String)> = conn
        .query(format!(
            "SELECT s.digest_text, s.digest, s.schemaname
            FROM stats_mysql_query_digest s 
            LEFT JOIN mysql_query_rules q 
            USING(digest) 
            WHERE s.hostgroup = {}
            AND s.username = '{}'
            AND s.schemaname NOT IN ('sys', 'information_schema', 'performance_schema', 'mysql')
            AND s.digest_text LIKE 'SELECT%FROM%'
            AND digest_text NOT LIKE '%?=?%'
            AND s.sum_rows_sent > 0
            AND q.rule_id IS NULL
            ORDER BY s.sum_rows_sent DESC",
            config.source_hostgroup, config.readyset_user
        ))
        .expect("Failed to query proxysql_conn");
    rows
}

pub fn replace_placeholders(query: &str) -> String {
    // date placeholder

    query.replace("?-?-?", "?")
}

pub fn check_readyset_query_support(
    conn: &mut PooledConn,
    digest_text: &String,
    schema: &String,
) -> Result<bool, mysql::Error> {
    conn.query_drop(format!("USE {}", schema))
        .expect("Failed to use schema");
    let row: Option<(String, String, String)> =
        conn.query_first(format!("EXPLAIN CREATE CACHE FROM {}", digest_text))?;
    match row {
        Some((_, _, value)) => Ok(value == "yes" || value == "cached"),
        None => Ok(false),
    }
}

pub fn cache_query(conn: &mut PooledConn, digest_text: &String) -> Result<bool, mysql::Error> {
    conn.query_drop(format!("CREATE CACHE FROM {}", digest_text))
        .expect("Failed to create readyset cache");
    Ok(true)
}

pub fn add_query_rule(
    conn: &mut PooledConn,
    digest: &String,
    config: &Config,
) -> Result<bool, mysql::Error> {
    let datetime_now: DateTime<Local> = Local::now();
    let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
    if config.warmup_time.is_some() {
        conn.query_drop(format!("INSERT INTO mysql_query_rules (username, mirror_hostgroup, active, digest, apply, comment) VALUES ('{}', {}, 1, '{}', 1, 'Mirror by readyset scheduler at: {}')", config.readyset_user, config.readyset_hostgroup, digest, date_formatted)).expect("Failed to insert into mysql_query_rules");
        messages::print_info("Inserted warm-up rule");
    } else {
        conn.query_drop(format!("INSERT INTO mysql_query_rules (username, destination_hostgroup, active, digest, apply, comment) VALUES ('{}', {}, 1, '{}', 1, 'Added by readyset scheduler at: {}')", config.readyset_user, config.readyset_hostgroup, digest, date_formatted)).expect("Failed to insert into mysql_query_rules");
        messages::print_info("Inserted destination rule");
    }
    Ok(true)
}

pub fn load_query_rules(conn: &mut PooledConn) -> Result<bool, mysql::Error> {
    conn.query_drop("LOAD MYSQL QUERY RULES TO RUNTIME")
        .expect("Failed to load query rules");
    Ok(true)
}
pub fn save_query_rules(conn: &mut PooledConn) -> Result<bool, mysql::Error> {
    conn.query_drop("SAVE MYSQL QUERY RULES TO DISK")
        .expect("Failed to load query rules");
    Ok(true)
}

pub fn adjust_mirror_rules(conn: &mut PooledConn, config: &Config) -> Result<bool, mysql::Error> {
    let mut updated_rules = false;
    let datetime_now: DateTime<Local> = Local::now();
    let tz = datetime_now.format("%z").to_string();
    let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
    let rows: Vec<(u16, String)> = conn.query("SELECT rule_id, comment FROM mysql_query_rules WHERE comment LIKE 'Mirror by readyset scheduler at: ____-__-__ __:__:__';").expect("Failed to select mirror rules");
    for (rule_id, comment) in rows {
        let datetime_mirror_str = comment
            .split("Mirror by readyset scheduler at:")
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
        if elapsed > config.warmup_time.unwrap() as i64 {
            let comment = format!(
                "{}\n Added by readyset scheduler at: {}",
                comment, date_formatted
            );
            conn.query_drop(format!("UPDATE mysql_query_rules SET mirror_hostgroup = NULL, destination_hostgroup = {}, comment = '{}' WHERE rule_id = {}", config.readyset_hostgroup, comment, rule_id)).expect("Failed to update rule");
            messages::print_info(
                format!("Updated rule ID {} from warmup to destination", rule_id).as_str(),
            );
            updated_rules = true;
        }
    }
    Ok(updated_rules)
}

pub fn query_discovery(
    proxysql_conn: &mut mysql::PooledConn,
    config: &config::Config,
    readyset_conn: &mut mysql::PooledConn,
) {
    let mut queries_added_or_change = adjust_mirror_rules(proxysql_conn, config).unwrap();

    let rows: Vec<(String, String, String)> = find_queries_to_cache(proxysql_conn, config);

    for (digest_text, digest, schema) in rows {
        let digest_text = replace_placeholders(&digest_text);
        messages::print_info(format!("Going to test query support for {}", digest_text).as_str());
        let supported = check_readyset_query_support(readyset_conn, &digest_text, &schema);
        match supported {
            Ok(true) => {
                messages::print_info(
                    "Query is supported, adding it to proxysql and readyset"
                        .to_string()
                        .as_str(),
                );
                queries_added_or_change = true;
                cache_query(readyset_conn, &digest_text).expect("Failed to create readyset cache");
                add_query_rule(proxysql_conn, &digest, config).expect("Failed to add query rule");
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
        load_query_rules(proxysql_conn).expect("Failed to load query rules");
        save_query_rules(proxysql_conn).expect("Failed to save query rules");
    }
}
