use super::Violation;
use super::util::*;
use crate::tree::{ancestors, has_child, named_children};
use tree_sitter::Node;

const QUEUE_ACTIVATION_REQUIRED: &[&str] = &["PROCEDURE_NAME", "MAX_QUEUE_READERS", "EXECUTE AS"];

const DATABASE_FILE_REQUIRED: &[&str] = &["FILENAME"];
const DATABASE_FILE_UNIQUE: &[&str] = &[
    "NAME",
    "SIZE",
    "OFFLINE",
    "FILENAME",
    "MAXSIZE",
    "FILEGROWTH",
];
const REPLICA_REQUIRED: &[&str] = &["ENDPOINT_URL", "AVAILABILITY_MODE", "FAILOVER_MODE"];
const SECONDARY_ROLE_UNIQUE: &[&str] = &["ALLOW_CONNECTIONS"];
const PRIMARY_ROLE_UNIQUE: &[&str] = &["READ_ONLY_ROUTING_LIST"];
const QUERY_STORE_UNIQUE: &[&str] = &[
    "OPERATION_MODE",
    "CLEANUP_POLICY",
    "STALE_QUERY_THRESHOLD_DAYS",
    "EXECUTION_COUNT",
];
const FILESTREAM_UNIQUE: &[&str] = &["DIRECTORY_NAME"];
const CHANGE_TRACKING_UNIQUE: &[&str] = &["AUTO_CLEANUP"];
const BUFFER_POOL_REQUIRED: &[&str] = &["FILENAME", "SIZE"];

/// CREATE/ALTER QUEUE ... WITH ACTIVATION (...): each setting at most once; CREATE also needs
/// PROCEDURE_NAME, MAX_QUEUE_READERS and EXECUTE AS. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, Msg 102 for oracle cases queue-duplicate-create/alter and
/// queue-missing-48/49/50, acceptance of queue-missing-51.
pub(super) fn queue_activation(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    for with_clause in named_children(statement)
        .into_iter()
        .filter(|n| n.kind() == "with_clause")
    {
        for activation in direct_options(with_clause)
            .into_iter()
            .filter(|n| option_name(*n, source) == "ACTIVATION")
        {
            let mut seen = std::collections::BTreeSet::new();
            let mut drop = false;
            for setting in named_children(activation) {
                let key = match setting.kind() {
                    "option" => match setting.child_by_field_name("name") {
                        Some(name) => upper_text(name, source),
                        None => continue,
                    },
                    "execute_as_clause" => "EXECUTE AS".into(),
                    "keyword_drop" => {
                        drop = true;
                        continue;
                    }
                    _ => continue, // the activation's own name
                };
                if !seen.insert(key.clone()) {
                    out.push(violation(
                        "queue-activation-duplicate",
                        format!("{key} appears more than once in ACTIVATION"),
                        setting,
                    ));
                }
            }
            if statement.kind() == "create_queue" && !drop {
                for &required in QUEUE_ACTIVATION_REQUIRED {
                    if !seen.contains(required) {
                        out.push(violation(
                            "queue-activation-required",
                            format!("CREATE QUEUE ACTIVATION needs {required}"),
                            activation,
                        ));
                    }
                }
            }
        }
    }
}

/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170: FILENAME missing is Msg 1036
/// in ddl-database/files/negative-suspected-003 and ddl-database/create/negative-suspected-001.
/// NAME/SIZE/OFFLINE repeated is Msg 153 in ddl-database/files/negative-suspected-004/005/007.
/// Do not require NAME: files/negative-suspected-002/006 are accepted, including MODIFY without it.
pub(super) fn database_file_options(file: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut required_filename = false;
    for statement in ancestors(file) {
        if matches!(statement.kind(), "create_database" | "alter_database") {
            let kinds: Vec<&str> = named_children(statement)
                .into_iter()
                .map(|n| n.kind())
                .collect();
            required_filename = if statement.kind() == "alter_database" {
                kinds.contains(&"keyword_add")
            } else {
                // Missing options in ATTACH/SNAPSHOT have not been measured by these probes.
                !kinds.iter().any(|k| {
                    matches!(
                        *k,
                        "keyword_attach" | "keyword_attach_rebuild_log" | "keyword_snapshot"
                    )
                })
            };
            break;
        }
    }
    option_set(
        file,
        source,
        "database-file-required",
        "database-file-duplicate",
        if required_filename {
            DATABASE_FILE_REQUIRED
        } else {
            &[]
        },
        // ddl-database/extra-file-duplicates/case-001..006: Msg 153, levels 100-170.
        DATABASE_FILE_UNIQUE,
        out,
    );
}

/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170: ddl-database/ag/negative-suspected-003,
/// 004 and 005 reject ADD REPLICA without ENDPOINT_URL, AVAILABILITY_MODE and FAILOVER_MODE
/// with Msg 35236, 41198 and 41197 respectively. Distributed JOIN has different required options.
/// CREATE's required options: ddl-database/edge/3..5 (normal REPLICA, not distributed groups).
/// ADD duplicates: ddl-database/extra-ag-option-sets/case-003..005, Msg 1039.
pub(super) fn availability_replica_options(replica: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(statement) = replica.parent().filter(|n| {
        matches!(
            n.kind(),
            "alter_availability_group" | "create_availability_group"
        )
    }) else {
        return;
    };
    // kit-routes/alt-tsql-B03-r040 and ddl-database/distributed/listener: Msg 155;
    // the corrected LISTENER_URL neighbour is accepted on LocalDB 17.0.1000.7, levels 100-170.
    if !has_child(statement, "keyword_replica") {
        for option in direct_options(replica) {
            if !matches!(
                option_name(option, source).as_str(),
                "LISTENER_URL" | "AVAILABILITY_MODE" | "FAILOVER_MODE" | "SEEDING_MODE"
            ) {
                out.push(violation(
                    "distributed-availability-option",
                    "unknown distributed availability-group option".into(),
                    option,
                ));
            }
        }
    }
    if (statement.kind() == "create_availability_group" && has_child(statement, "keyword_replica"))
        || has_child(statement, "keyword_add")
    {
        option_set(
            replica,
            source,
            "availability-replica-required",
            "availability-replica-duplicate",
            REPLICA_REQUIRED,
            REPLICA_REQUIRED,
            out,
        );
    }
    // ddl-database/extra-ag-option-sets/case-010: MODIFY takes one outer option, Msg 102.
    if statement.kind() == "alter_availability_group"
        && has_child(statement, "keyword_modify")
        && has_child(statement, "keyword_replica")
        && direct_options(replica).len() != 1
    {
        out.push(violation(
            "availability-modify-single-option",
            "MODIFY REPLICA takes one outer option".into(),
            replica,
        ));
    }
    // ddl-database/extra-ag-option-sets/case-006/008: duplicate role settings, Msg 1039.
    for role in direct_options(replica) {
        let unique: &[&str] = match option_name(role, source).as_str() {
            "SECONDARY_ROLE" => SECONDARY_ROLE_UNIQUE,
            "PRIMARY_ROLE" => PRIMARY_ROLE_UNIQUE,
            _ => continue,
        };
        option_set(
            role,
            source,
            "availability-role-required",
            "availability-role-duplicate",
            &[],
            unique,
            out,
        );
        // ddl-database/edge/6 accepts one role setting; edge/7/8 reject two distinct settings, Msg 10786.
        if has_child(statement, "keyword_modify")
            && has_child(statement, "keyword_replica")
            && direct_options(role).len() != 1
        {
            out.push(violation(
                "availability-modify-role-single-option",
                "MODIFY REPLICA role takes one nested option".into(),
                role,
            ));
        }
    }
}

/// ddl-database/extra-ag-option-sets/case-001/002 reject two SET options, even distinct (Msg 102).
pub(super) fn availability_set_options(statement: Node, out: &mut Vec<Violation>) {
    if has_child(statement, "keyword_set") && direct_options(statement).len() != 1 {
        out.push(violation(
            "availability-set-single-option",
            "ALTER AVAILABILITY GROUP SET takes one option".into(),
            statement,
        ));
    }
}

/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170:
/// ddl-database/extra-query-store/case-001..004: Msg 12401 for repeated nested settings;
/// extra-database-set/case-003: FILESTREAM DIRECTORY_NAME Msg 102; case-004: AUTO_CLEANUP Msg 5091.
/// Top-level AUTO_CLOSE/RECOVERY repeats (case-001/002) are accepted and intentionally not checked.
pub(super) fn nested_database_options(option: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut query_store = false;
    let mut database = false;
    for n in std::iter::once(option).chain(ancestors(option)) {
        query_store |= n.kind() == "option" && option_name(n, source) == "QUERY_STORE";
        if matches!(n.kind(), "create_database" | "alter_database") {
            database = true;
            break;
        }
    }
    if !database {
        return;
    }
    // extra-query-store/case-006/007: Msg 102 for MINUTES or an unknown policy key;
    // edge/13 and extra-query-store/case-005 accept HOURS and DAYS at levels 100-170.
    if query_store && option_name(option, source) == "QUERY_CAPTURE_POLICY" {
        for setting in direct_options(option) {
            let name = option_name(setting, source);
            let valid = match name.as_str() {
                "STALE_CAPTURE_POLICY_THRESHOLD" => setting
                    .child_by_field_name("value")
                    .and_then(|value| value.syntax_next_named_sibling())
                    .is_some_and(|n| matches!(upper_text(n, source).as_str(), "HOURS" | "DAYS")),
                "EXECUTION_COUNT" | "TOTAL_COMPILE_CPU_TIME_MS" | "TOTAL_EXECUTION_CPU_TIME_MS" => {
                    true
                }
                _ => false,
            };
            if !valid {
                out.push(violation(
                    "query-capture-policy-option",
                    format!("invalid QUERY_CAPTURE_POLICY setting {name}"),
                    setting,
                ));
            }
        }
    }
    let unique: &[&str] = if query_store {
        QUERY_STORE_UNIQUE
    } else {
        match option_name(option, source).as_str() {
            "FILESTREAM" => FILESTREAM_UNIQUE,
            "CHANGE_TRACKING" => CHANGE_TRACKING_UNIQUE,
            _ => return,
        }
    };
    option_set(
        option,
        source,
        "database-option-required",
        "database-option-duplicate",
        &[],
        unique,
        out,
    );
}

/// ddl-database/extra-audit-clauses/case-002/003 reject repeated TO/WITH clauses (Msg 156/319).
/// case-006 accepts repeated STATE inside one WITH list, so options inside it are not checked.
pub(super) fn audit_clause_count(statement: Node, out: &mut Vec<Violation>) {
    let mut seen = std::collections::BTreeSet::new();
    for child in named_children(statement) {
        if !matches!(child.kind(), "keyword_to" | "with_options") {
            continue;
        }
        if !seen.insert(child.kind()) {
            out.push(violation(
                "server-audit-duplicate-clause",
                format!("{} appears more than once", child.kind()),
                child,
            ));
        }
    }
}

/// ddl-database/extra-buffer-pool/case-001..006: Msg 102 for duplicates, bare keys, reverse order
/// and an identifier FILENAME. The accepted server/positive-011 has FILENAME then numeric SIZE.
/// ddl-database/edge/9..11 reject missing unit, quoted size and TB, respectively (Msg 102).
pub(super) fn buffer_pool_options(options: Node, source: &[u8], out: &mut Vec<Violation>) {
    option_set(
        options,
        source,
        "server-buffer-pool-required",
        "server-buffer-pool-duplicate",
        BUFFER_POOL_REQUIRED,
        BUFFER_POOL_REQUIRED,
        out,
    );
    let settings = direct_options(options);
    if settings.len() == 2
        && option_name(settings[0], source) == "SIZE"
        && option_name(settings[1], source) == "FILENAME"
    {
        out.push(violation(
            "server-buffer-pool-order",
            "FILENAME must precede SIZE".into(),
            options,
        ));
    }
    for setting in settings {
        let name = option_name(setting, source);
        let value = setting.child_by_field_name("value");
        let valid = value.is_some_and(|v| {
            let text = upper_text(v, source);
            match name.as_str() {
                "FILENAME" => {
                    v.kind() == "literal" && (text.starts_with('\'') || text.starts_with("N'"))
                }
                "SIZE" => {
                    v.kind() == "literal"
                        && !text.is_empty()
                        && text.bytes().all(|b| b.is_ascii_digit())
                        && v.syntax_next_named_sibling().is_some_and(|unit| {
                            matches!(upper_text(unit, source).as_str(), "KB" | "MB" | "GB")
                        })
                }
                _ => true,
            }
        });
        if !has_child(setting, "=") || !valid {
            out.push(violation(
                "server-buffer-pool-value",
                format!(
                    "{name} needs an assigned {}",
                    if name == "FILENAME" {
                        "string"
                    } else {
                        "numeric value"
                    }
                ),
                setting,
            ));
        }
    }
}

/// ddl-database/extra-encryption-options/case-001..004/006..009: Msg 102 for bare, wrong-name,
/// unknown or repeated ALGORITHM; security-ddl/positive-001..006 accept the four algorithms.
pub(super) fn encryption_algorithm(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    for clause in named_children(statement)
        .into_iter()
        .filter(|n| n.kind() == "with_clause")
    {
        let options = direct_options(clause);
        let valid = options.len() == 1
            && option_name(options[0], source) == "ALGORITHM"
            && has_child(options[0], "=")
            && options[0].child_by_field_name("value").is_some_and(|v| {
                matches!(
                    upper_text(v, source).as_str(),
                    "AES_128" | "AES_192" | "AES_256" | "TRIPLE_DES_3KEY"
                )
            });
        if !valid {
            out.push(violation(
                "database-encryption-algorithm",
                "WITH needs one assigned ALGORITHM: AES_128, AES_192, AES_256 or TRIPLE_DES_3KEY"
                    .into(),
                clause,
            ));
        }
    }
}
