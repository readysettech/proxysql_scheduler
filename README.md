# Readyset ProxySQL Scheduler
Scheduler to integrate Readyset and ProxySQL. 

# Workflow
This scheduler executes the following steps:

1. Locks an in disk file (configured by `lock_file`) to avoid multiple instances of the scheduler to overlap their execution.
2. Queries the table `stats_mysql_query_digest` from ProxySQL and validates if each query is supported by Readyset
3. If the query is supported it adds a cache in Readyset by executing `CREATE CACHE FROM __query__`.
4. If `warmup_time` is NOT configure, a new query rule will be added redirecting this query to Readyset
5. If `warmup_time` is configured, a new query rule will be added to mirror this query to readyset. The query will still be redirected to the original hostgroup
6. Once `warmup_time` seconds has elapsed since the query was mirrored, the query rule will be updated to redirect the qury to Readyset instead of mirroring.



# Configuration

Assuming you have your ProxySQL already Configured you will need to create a new hostgroup and add Readyset to this hostgroup:

```
INSERT INTO mysql_servers (hostgroup_id, hostname, port) VALUES (99, '127.0.0.1', 3307);
LOAD MYSQL SERVERS TO RUNTIME;
SAVE MYSQL SERVERS TO DISK;
```

To configure the scheduler to run execute:

```
INSERT INTO scheduler (active, interval_ms, filename, arg1) VALUES (1, 10000, '/usr/bin/readyset_proxysql_scheduler', '--config=/etc/readyset_proxysql_scheduler.cnf');
LOAD SCHEDULER TO RUNTIME;
SAVE SCHEDULER TO DISK;
```

Configure `/etc/readyset_proxysql_scheduler.cnf` as follow:
* `proxysql_user` - (Required) - Proxysql admin user
* `proxysql_password` - (Required) - Proxysql admin password
* `proxysql_host` - (Required) - Proxysql admin host
* `proxysql_port` - (Required) - Proxysql admin port
* `readyset_user` - (Required) - Readyset application user
* `readyset_password` - (Required) - Readyset application password
* `readyset_host` - (Required) - Readyset host
* `readyset_port` - (Required) - Readyset port
* `source_hostgroup` - (Required) - Hostgroup running your Read workload
* `readyset_hostgroup` - (Required) - Hostgroup where Readyset is configure
* `warmup_time` - (Optional) - Time in seconds to mirror a query supported before redirecting the query to Readyset (Default 0 - no mirror)
* `lock_file` - (Optional) - Lock file to prevent two instances of the scheduler to run at the same time (Default '/etc/readyset_scheduler.lock')


# Note
Readyset support of MySQL and this scheduler are alpha quality, meaning they are not currently part of our test cycle. Run your own testing before plugging this to your production system.