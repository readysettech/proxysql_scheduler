# Automatic Query Caching with Readyset ProxySQL Scheduler
Unlock the full potential of your database integrating Readyset and ProxySQL by automatically analyzing and caching inefficient queries in your workload. Experience optimized performance with zero code changes—your application runs faster, effortlessly.


# Workflow
This scheduler executes the following steps:

1. Locks an in disk file (configured by `lock_file`) to avoid multiple instances of the scheduler to overlap their execution.
2. If `operation_mode=("All"|"HealthCheck")` -  Query `mysql_servers` and check all servers that have `comment='Readyset` (case insensitive) and `hostgroup=readyset_hostgroup`. For each server it checks if it can connect to Readyset and validate the output of  `Status` and act as follow:
   * `Online` - Adjust the server status to `ONLINE` in ProxySQL.
   * `Maitenance Mode` -  Adjust the server status to `OFFLINE_SOFT` in ProxySQL.
   * `Snapshot In Progress` - Adjust the server status to `SHUNNED` in ProxySQL.
4. If `operation_mode=("All"|"QueryDiscovery")` Query the table `stats_mysql_query_digest` finding queries executed at `source_hostgroup` by `readyset_user` and validates if each query is supported by Readyset. The rules to order queries are configured by [Query Discovery](#query-discovery) configurations.
3. If the query is supported it adds a cache in Readyset by executing `CREATE CACHE FROM __query__`.
4. If `warmup_time_s` is NOT configure, a new query rule will be added redirecting this query to Readyset
5. If `warmup_time_s` is configured, a new query rule will be added to mirror this query to Readyset. The query will still be redirected to the original hostgroup
6. Once `warmup_time_s` seconds has elapsed since the query was mirrored, the query rule will be updated to redirect the query to Readyset instead of mirroring.



# Configuration

Assuming you have your ProxySQL already Configured you will need to create a new hostgroup and add Readyset to this hostgroup:

```
INSERT INTO mysql_servers (hostgroup_id, hostname, port, comment) VALUES (99, '127.0.0.1', 3307, 'Readyset');
LOAD MYSQL SERVERS TO RUNTIME;
SAVE MYSQL SERVERS TO DISK;
```

*NOTE*: It's required to add `Readyset` as a comment to the server to be able to identify it in the scheduler.

To configure the scheduler to run execute:

```
INSERT INTO scheduler (active, interval_ms, filename, arg1) VALUES (1, 10000, '/usr/bin/readyset_proxysql_scheduler', '--config=/etc/readyset_proxysql_scheduler.cnf');
LOAD SCHEDULER TO RUNTIME;
SAVE SCHEDULER TO DISK;
```

Configure `/etc/readyset_proxysql_scheduler.cnf` as follow:
* `database_type` - (Optional) - Either `"mysql"` or `"postgresql"` (Default `"mysql"`)
* `proxysql_user` - (Required) - ProxySQL admin user
* `proxysql_password` - (Required) - ProxySQL admin password
* `proxysql_host` - (Required) - ProxySQL admin host
* `proxysql_port` - (Required) - ProxySQL admin port
* `readyset_user` - (Required) - Readyset application user
* `readyset_password` - (Required) - Readyset application password
* `source_hostgroup` - (Required) - Hostgroup running your Read workload
* `readyset_hostgroup` - (Required) - Hostgroup where Readyset is configure
* `warmup_time_s` - (Optional) - Time in seconds to mirror a query supported before redirecting the query to Readyset (Default `0` - no mirror)
* `lock_file` - (Optional) - Lock file to prevent two instances of the scheduler to run at the same time (Default `"/etc/readyset_scheduler.lock"`)
* `operation_mode` - (Optional) - Operation mode to run the scheduler. The options are described in [Operation Mode](#operation-mode) (Default `"All"`).
* `number_of_queries` - (Optional) - Number of queries to cache in Readyset (Default `10`).
* `query_discovery_mode` / `query_discovery_min_execution` / `query_discovery_min_row_sent` - (Optional) - Query Discovery configurations. The options are described in [Query Discovery](#query-discovery) (Default `"CountStar"` / `0` / `0`).


# Query Discovery
The Query Discovery is a set of configuration to find queries that are supported by Readyset. The configurations are defined by the following fields:

* `query_discovery_mode`: (Optional) - Mode to discover queries to automatically cache in Readyset. The options are described in [Query Discovery Mode](#query-discovery-mode) (Default `"CountStar"`).
* `query_discovery_min_execution`: (Optional) - Minimum number of executions of a query to be considered a candidate to be cached (Default `0`).
* `query_discovery_min_row_sent`: (Optional) - Minimum number of rows sent by a query to be considered a candidate to be cached (Default `0`).

# Query Discovery Mode
The Query Discovery Mode is a set of possible rules to discover queries to automatically cache in Readyset. The options are:

1. `"CountStar"` - Total Number of Query Executions
 * Formula: `total_executions = count_star`
 * Description: This metric gives the total number of times the query has been executed. It is valuable for understanding how frequently the query runs. A high count_star value suggests that the query is executed often.

2. `"SumTime"` - Total Time Spent Executing the Query
 * Formula: `total_execution_time = sum_time`
 * Description: This metric represents the total cumulative time spent (measured in microseconds) executing the query across all its executions. It provides a clear understanding of how much processing time the query is consuming over time. A high total execution time can indicate that the query is either frequently executed or is time-intensive to process.

3. `"SumRowsSent"` - Total Number of Rows Sent by the Query (sum_rows_sent)
 * Formula: `total_rows_sent = sum_rows_sent`
 * Description: This metric provides the total number of rows sent to the client across all executions of the query. It helps you understand the query’s output volume and the amount of data being transmitted.

4. `"MeanTime"` - Average Query Execution Time (Mean)
 * Formula: `mean_time = sum_time / count_star`
 * Description: The mean time gives you an idea of the typical performance (measured in microseconds) of the query over all executions. It provides a central tendency of how long the query generally takes to execute.

5. `"ExecutionTimeDistance"` - Time Distance Between Query Executions
 * Formula: `execution_time_distance = max_time - min_time`
 * Description: This shows the spread between the fastest and slowest executions of the query (measured in microseconds). A large range might indicate variability in system load, input sizes, or external factors affecting performance.

6. `"QueryThroughput"` - Query Throughput
 * Formula: `query_throughput = count_star / sum_time`
 * Description: This shows how many queries are processed per unit of time (measured in microseconds). It’s useful for understanding system capacity and how efficiently the database is handling the queries.

7. `"WorstBestCase"` - Worst Best-Case Query Performance
 * Formula: `worst_case = max(min_time)`
 * Description: The min_time metric gives the fastest time the query was ever executed (measured in microseconds). It reflects the best-case performance scenario, which could indicate the query’s performance under optimal conditions.

8. `"WorstWorstCase"` - Worst Worst-Case Query Performance
 * Formula: `worst_case = max(max_time)`
 * Description: The max_time shows the slowest time the query was executed (measured in microseconds). This can indicate potential bottlenecks or edge cases where the query underperforms, which could be due to larger data sets, locks, or high server load.

9. `"DistanceMeanMax"` - Distance Between Mean Time and Max Time (mean_time vs max_time)
 * Formula: `distance_mean_max = max_time - mean_time`
 * Description: The distance between the mean execution time and the maximum execution time provides insight into how much slower the worst-case execution is compared to the average (measured in microseconds). A large gap indicates significant variability in query performance, which could be caused by certain executions encountering performance bottlenecks, such as large datasets, locking, or high system load.

# Operation Mode
The Operation Mode is a set of possible rules to run the scheduler. The options are:
* `All` - Run `HealthCheck` and `QueryDiscovery` operations.
* `HealthCheck` - Run only the health check operation.
* `QueryDiscovery` - Run only the query discovery operation.
