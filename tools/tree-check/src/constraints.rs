//! Option-set constraints that SQL Server enforces while parsing but the grammar does not encode
//! (ADR-0003): an option at most once, required options, mutually exclusive options. Each rule
//! cites the engine evidence it was measured against.

use tree_sitter::{Node, Tree};

/// A constraint violation: the rule and the row/column (0-based) of the offending node.
#[derive(Debug, PartialEq)]
pub struct Violation {
    pub rule: &'static str,
    pub detail: String,
    pub row: usize,
    pub column: usize,
}

pub fn check(tree: &Tree, source: &[u8]) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if matches!(node.kind(), "create_queue" | "alter_queue") {
            queue_activation(node, source, &mut out);
        }
        match node.kind() {
            "create_endpoint" | "alter_endpoint" => endpoint_options(node, source, &mut out),
            "create_fulltext_catalog" | "create_fulltext_index" => {
                fulltext_options(node, source, &mut out)
            }
            "create_external_data_source"
            | "alter_external_data_source"
            | "create_external_model"
            | "alter_external_model"
            | "create_external_language"
            | "alter_external_language"
            | "create_external_library"
            | "alter_external_library"
            | "create_external_resource_pool"
            | "alter_external_resource_pool" => external_options(node, source, &mut out),
            "statement" => {
                cte_terminator(node, &mut out);
                cte_table_source(node, &mut out);
            }
            "grant_statement" => column_permissions(node, source, &mut out),
            "create_trigger" | "alter_trigger" => trigger_scope(node, source, &mut out),
            "invocation" => builtin_arity(node, source, &mut out),
            "create_function" | "alter_function" => scalar_function_return(node, &mut out),
            "window_specification" => window_frame_order(node, &mut out),
            "merge" => merge_terminator(node, &mut out),
            "table_hint" | "query_hint" => table_hints(node, source, &mut out),
            "option_clause" => table_hint_aliases(node, source, &mut out),
            "database_file" => database_file_options(node, source, &mut out),
            "availability_replica" => availability_replica_options(node, source, &mut out),
            "server_buffer_pool_options" => buffer_pool_options(node, source, &mut out),
            "alter_availability_group" => availability_set_options(node, &mut out),
            "option" => nested_database_options(node, source, &mut out),
            "alter_server_audit" => audit_clause_count(node, &mut out),
            "create_database_encryption_key" | "alter_database_encryption_key" => {
                encryption_algorithm(node, source, &mut out)
            }
            _ => {}
        }
        if node.kind() == "execute_statement" {
            exec_output_requires_at(node, &mut out);
            exec_output_variable(node, source, &mut out);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return out;
            }
        }
    }
}

/// A statement that starts with a CTE must not directly follow another statement or block: the
/// previous one needs a `;`. It may come first in a batch, block or module body, right after
/// `;`, a label, or as the body of IF, ELSE or WHILE. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, Msg 319 for the semicolon/*-no-terminator probes (after SELECT,
/// END, IF, IF ... ELSE, WHILE and inside a procedure body), acceptance of every terminated,
/// leading-semicolon, label, IF/ELSE/WHILE-body and first-in-block form.
fn cte_terminator(statement: Node, out: &mut Vec<Violation>) {
    // `WITH c AS (...) SELECT ...` is a statement whose children start with `keyword_with`, `cte`.
    let starts_with_cte = statement.named_child(0).map(|n| n.kind()) == Some("keyword_with")
        && statement.named_child(1).map(|n| n.kind()) == Some("cte");
    if !starts_with_cte {
        return;
    }
    let Some(prev) = statement.prev_sibling() else {
        return;
    };
    let follows_statement = match prev.kind() {
        "block" => true,
        "statement" => prev.named_child(0).map(|n| n.kind()) != Some("label_statement"),
        _ => false,
    };
    if follows_statement {
        out.push(violation(
            "cte-terminator",
            "the statement before a WITH common table expression must end with ';'".into(),
            statement,
        ));
    }
}

/// `EXEC ( string [, value [OUTPUT]] ... )`: a value marked OUTPUT or OUT is a parameter passed to a
/// linked server, which needs `AT linked_server`. Evidence: SQL Server 2025 LocalDB 17.0.1000.7,
/// levels 100-170: accepted with AT (OUTPUT, OUT, several), Msg 102 without AT. (A constant marked
/// OUTPUT is Msg 179, a context error left to grammar-kit.) Procedure calls have their own
/// `exec_argument` nodes and are not affected.
fn exec_output_requires_at(statement: Node, out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.children(&mut cursor).collect();
    if children.iter().any(|n| n.kind() == "keyword_at") {
        return;
    }
    if let Some(output) = children
        .iter()
        .find(|n| matches!(n.kind(), "keyword_output" | "keyword_out"))
    {
        out.push(violation(
            "exec-output-requires-at",
            "an OUTPUT parameter of EXEC ( ... ) needs AT linked_server".into(),
            *output,
        ));
    }
}

/// OUTPUT or OUT must mark a local variable (`@v`, not `@@v`), in a procedure call's arguments and in
/// `EXEC ( ... ) AT`. Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170: Msg 179 for an
/// integer, a string, DEFAULT, a system variable or a bare name marked OUTPUT (positional and named),
/// Msg 102 for an expression in `EXEC ( ... ) AT`; accepted for `@v OUTPUT`, `@v OUT`, `@a = @v OUTPUT`.
fn exec_output_variable(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let is_output = |n: &Node| matches!(n.kind(), "keyword_output" | "keyword_out");
    let local_variable = |n: Node| {
        let name = if n.kind() == "field" && n.named_child_count() == 1 {
            n.child_by_field_name("name")
        } else {
            Some(n)
        };
        name.filter(|n| n.kind() == "identifier")
            .and_then(|n| n.utf8_text(source).ok())
            .is_some_and(|text| text.starts_with('@') && !text.starts_with("@@"))
    };
    let mut cursor = statement.walk();
    for child in statement.children(&mut cursor).collect::<Vec<_>>() {
        // EXEC ( string, value OUTPUT ) AT: the value is the named node right before OUTPUT.
        if is_output(&child) {
            if !child.prev_named_sibling().is_some_and(local_variable) {
                out.push(violation(
                    "exec-output-variable",
                    "OUTPUT must mark a local variable".into(),
                    child,
                ));
            }
            continue;
        }
        if child.kind() == "exec_argument" {
            let mut c = child.walk();
            let marked = child.children(&mut c).any(|n| is_output(&n));
            if marked
                && !child
                    .child_by_field_name("value")
                    .is_some_and(local_variable)
            {
                out.push(violation(
                    "exec-output-variable",
                    "OUTPUT must mark a local variable".into(),
                    child,
                ));
            }
        }
    }
}

/// A column list on a permission (`GRANT p (a) ON t`, or `ON t (a)` for every permission) allows
/// only SELECT, UPDATE, REFERENCES, UNMASK and ALL [PRIVILEGES]. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, oracle set `context/perm-*` and the permission-* cases: Msg 1020 for
/// INSERT, DELETE, EXECUTE, ALTER, CONTROL, TAKE OWNERSHIP, VIEW DEFINITION, VIEW CHANGE TRACKING
/// with columns, in GRANT, DENY and REVOKE [GRANT OPTION FOR]; accepted for the allowed ones.
fn column_permissions(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.children(&mut cursor).collect();
    let on_object = children.iter().any(|n| n.kind() == "(");
    for permission in children.iter().filter(|n| n.kind() == "permission") {
        let mut c = permission.walk();
        let parts: Vec<Node> = permission.named_children(&mut c).collect();
        if !on_object && !parts.iter().any(|n| n.kind() == "list") {
            continue;
        }
        let words: Vec<String> = parts
            .iter()
            .filter(|n| n.kind() != "list")
            .map(|n| upper_text(*n, source))
            .collect();
        let words = words.join(" ");
        if !matches!(
            words.as_str(),
            "SELECT" | "UPDATE" | "REFERENCES" | "UNMASK" | "ALL" | "ALL PRIVILEGES"
        ) {
            out.push(violation(
                "column-permission",
                format!("{words} cannot be granted on columns"),
                *permission,
            ));
        }
    }
}

/// Events that only a server-scoped trigger (`ON ALL SERVER`) can use: every event type of
/// sys.trigger_event_types that `ON DATABASE` rejects with Msg 1098 and `ON ALL SERVER` accepts,
/// measured on SQL Server 2025 LocalDB 17.0.1000.7 (oracle set `context/trig-db-*`, `trig-srv-*`).
const SERVER_ONLY_EVENTS: &[&str] = &[
    "ADD_SERVER_ROLE_MEMBER",
    "ALTER_AUTHORIZATION_SERVER",
    "ALTER_AVAILABILITY_GROUP",
    "ALTER_CREDENTIAL",
    "ALTER_CRYPTOGRAPHIC_PROVIDER",
    "ALTER_DATABASE",
    "ALTER_ENDPOINT",
    "ALTER_EVENT_SESSION",
    "ALTER_EXTERNAL_RESOURCE_POOL",
    "ALTER_INSTANCE",
    "ALTER_LINKED_SERVER",
    "ALTER_LOGIN",
    "ALTER_MESSAGE",
    "ALTER_POOL",
    "ALTER_REMOTE_SERVER",
    "ALTER_RESOURCE_GOVERNOR_CONFIG",
    "ALTER_RESOURCE_POOL",
    "ALTER_SERVER_AUDIT",
    "ALTER_SERVER_AUDIT_SPECIFICATION",
    "ALTER_SERVER_ROLE",
    "ALTER_SERVICE_MASTER_KEY",
    "ALTER_SYNAPSE_WLC",
    "ALTER_SYNAPSE_WLG",
    "ALTER_WORKLOAD_GROUP",
    "CREATE_AVAILABILITY_GROUP",
    "CREATE_CREDENTIAL",
    "CREATE_CRYPTOGRAPHIC_PROVIDER",
    "CREATE_DATABASE",
    "CREATE_ENDPOINT",
    "CREATE_EVENT_SESSION",
    "CREATE_EXTENDED_PROCEDURE",
    "CREATE_EXTERNAL_RESOURCE_POOL",
    "CREATE_LINKED_SERVER",
    "CREATE_LINKED_SERVER_LOGIN",
    "CREATE_LOGIN",
    "CREATE_MESSAGE",
    "CREATE_POOL",
    "CREATE_REMOTE_SERVER",
    "CREATE_RESOURCE_POOL",
    "CREATE_SERVER_AUDIT",
    "CREATE_SERVER_AUDIT_SPECIFICATION",
    "CREATE_SERVER_ROLE",
    "CREATE_SYNAPSE_WLC",
    "CREATE_SYNAPSE_WLG",
    "CREATE_WORKLOAD_GROUP",
    "DDL_AUTHORIZATION_SERVER_EVENTS",
    "DDL_AVAILABILITY_GROUP_EVENTS",
    "DDL_CREDENTIAL_EVENTS",
    "DDL_CRYPTOGRAPHIC_PROVIDER_EVENTS",
    "DDL_DATABASE_EVENTS",
    "DDL_ENDPOINT_EVENTS",
    "DDL_EVENTS",
    "DDL_EVENT_SESSION_EVENTS",
    "DDL_EXTENDED_PROCEDURE_EVENTS",
    "DDL_EXTERNAL_RESOURCE_POOL_EVENTS",
    "DDL_GDR_SERVER_EVENTS",
    "DDL_LINKED_SERVER_EVENTS",
    "DDL_LINKED_SERVER_LOGIN_EVENTS",
    "DDL_LOGIN_EVENTS",
    "DDL_MESSAGE_EVENTS",
    "DDL_REMOTE_SERVER_EVENTS",
    "DDL_RESOURCE_GOVERNOR_EVENTS",
    "DDL_RESOURCE_POOL",
    "DDL_SERVER_AUDIT_EVENTS",
    "DDL_SERVER_AUDIT_SPECIFICATION_EVENTS",
    "DDL_SERVER_LEVEL_EVENTS",
    "DDL_SERVER_SECURITY_EVENTS",
    "DDL_SERVICE_MASTER_KEY_EVENTS",
    "DDL_SYNAPSE_POOL_EVENTS",
    "DDL_SYNAPSE_WLC_EVENTS",
    "DDL_SYNAPSE_WLG_EVENTS",
    "DDL_WORKLOAD_GROUP",
    "DENY_SERVER",
    "DROP_AVAILABILITY_GROUP",
    "DROP_CREDENTIAL",
    "DROP_CRYPTOGRAPHIC_PROVIDER",
    "DROP_DATABASE",
    "DROP_ENDPOINT",
    "DROP_EVENT_SESSION",
    "DROP_EXTENDED_PROCEDURE",
    "DROP_EXTERNAL_RESOURCE_POOL",
    "DROP_LINKED_SERVER",
    "DROP_LINKED_SERVER_LOGIN",
    "DROP_LOGIN",
    "DROP_MESSAGE",
    "DROP_POOL",
    "DROP_REMOTE_SERVER",
    "DROP_RESOURCE_POOL",
    "DROP_SERVER_AUDIT",
    "DROP_SERVER_AUDIT_SPECIFICATION",
    "DROP_SERVER_ROLE",
    "DROP_SERVER_ROLE_MEMBER",
    "DROP_SYNAPSE_WLC",
    "DROP_SYNAPSE_WLG",
    "DROP_WORKLOAD_GROUP",
    "GRANT_SERVER",
    "LOGON",
    "REVOKE_SERVER",
];

/// Trigger scope, decided by `ON DATABASE` / `ON ALL SERVER` (a DDL trigger) versus `ON object`
/// (a DML trigger). Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set
/// `context/trig-*` and the trigger-* cases, for CREATE, ALTER and CREATE OR ALTER:
/// - a DDL trigger name has no schema (Msg 1094);
/// - a DDL trigger takes no INSERT/UPDATE/DELETE and a DML trigger nothing else (Msg 1098);
/// - `ON DATABASE` takes no server-only event (Msg 1098); ALTER_SERVER_CONFIGURATION is
///   rejected on both scopes (Msg 1082);
/// - a DDL trigger cannot `EXECUTE AS OWNER` (Msg 1083).
fn trigger_scope(trigger: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = trigger.walk();
    let children: Vec<Node> = trigger.children(&mut cursor).collect();
    let database = children.iter().any(|n| n.kind() == "keyword_database");
    let ddl = database || children.iter().any(|n| n.kind() == "keyword_server");
    if let Some(name) = trigger
        .child_by_field_name("name")
        .filter(|n| ddl && n.child_by_field_name("schema").is_some())
    {
        out.push(violation(
            "trigger-scope",
            "a DATABASE or ALL SERVER trigger name cannot have a schema".into(),
            name,
        ));
    }
    for event in children.iter().filter(|n| n.kind() == "trigger_event") {
        let name = upper_text(*event, source);
        let dml = matches!(name.as_str(), "INSERT" | "UPDATE" | "DELETE");
        let detail = if ddl == dml {
            format!("{name} is not an event of this kind of trigger")
        } else if database && SERVER_ONLY_EVENTS.contains(&name.as_str()) {
            format!("{name} is a server event; it needs ON ALL SERVER")
        } else if name == "ALTER_SERVER_CONFIGURATION" {
            format!("{name} cannot fire a trigger")
        } else {
            continue;
        };
        out.push(violation("trigger-scope", detail, *event));
    }
    if !ddl {
        return;
    }
    for options in children.iter().filter(|n| n.kind() == "trigger_options") {
        let mut stack = vec![*options];
        while let Some(n) = stack.pop() {
            let mut c = n.walk();
            if n.kind() == "execute_as_clause"
                && n.children(&mut c).any(|k| k.kind() == "keyword_owner")
            {
                out.push(violation(
                    "trigger-scope",
                    "a DATABASE or ALL SERVER trigger cannot EXECUTE AS OWNER".into(),
                    n,
                ));
            }
            let mut c = n.walk();
            stack.extend(n.named_children(&mut c));
        }
    }
}

/// Built-in functions with a fixed number of arguments: the one-argument aggregates (`*` counts as
/// the argument of COUNT and COUNT_BIG), ABS, LEN, and the two-argument ISNULL and STRING_AGG. A
/// schema-qualified or bracketed name is not checked. Evidence: SQL Server 2025 LocalDB 17.0.1000.7,
/// levels 100-170, oracle sets `context/count-*`, `context/arity-*` and the count-* cases: Msg 174
/// for any other count (Msg 102 for some shapes); accepted otherwise.
fn builtin_arity(invocation: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(name) = invocation
        .named_child(0)
        .filter(|n| n.kind() == "object_reference" && n.named_child_count() == 1)
    else {
        return;
    };
    let name = upper_text(name, source);
    let expected = match name.as_str() {
        "COUNT"
        | "COUNT_BIG"
        | "AVG"
        | "MIN"
        | "MAX"
        | "SUM"
        | "STDEV"
        | "STDEVP"
        | "VAR"
        | "VARP"
        | "CHECKSUM_AGG"
        | "GROUPING"
        | "APPROX_COUNT_DISTINCT"
        | "ABS"
        | "LEN" => 1,
        "ISNULL" | "STRING_AGG" => 2,
        _ => return,
    };
    let mut c = invocation.walk();
    let given = invocation
        .children_by_field_name("parameter", &mut c)
        .count();
    if given != expected {
        out.push(violation(
            "builtin-arity",
            format!("{name} takes {expected} argument(s), not {given}"),
            invocation,
        ));
    }
}

/// A scalar function (RETURNS a type, not TABLE) must RETURN a value everywhere in its body.
/// Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set `context/return-*`
/// and the scalar-return-* cases: Msg 1075 for a bare RETURN at any depth, in CREATE and ALTER;
/// accepted in multi-statement table functions and procedures.
fn scalar_function_return(function: Node, out: &mut Vec<Violation>) {
    let mut cursor = function.walk();
    if function
        .children(&mut cursor)
        .any(|n| matches!(n.kind(), "keyword_table" | "table_var_declaration"))
    {
        return;
    }
    let Some(body) = function.child_by_field_name("body") else {
        return;
    };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "return_statement" && n.named_child_count() == 1 {
            out.push(violation(
                "scalar-function-return",
                "RETURN in a scalar function needs a value".into(),
                n,
            ));
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

/// `WITH c AS (...) SELECT ...` with no table source outside the CTE definitions (no FROM anywhere
/// in the query, subqueries included). A set operation is exempt. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, oracle set `context/cte-*` and cte-none: Msg 422 for a FROM-less
/// SELECT (also with two CTEs); accepted with FROM in the query or a subquery, for UNION, and for
/// INSERT and UPDATE.
fn cte_table_source(statement: Node, out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.named_children(&mut cursor).collect();
    if children.first().map(|n| n.kind()) != Some("keyword_with")
        || !children.iter().any(|n| n.kind() == "query_specification")
    {
        return;
    }
    let mut stack: Vec<Node> = children.into_iter().filter(|n| n.kind() != "cte").collect();
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "from" | "relation") {
            return;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    out.push(violation(
        "cte-table-source",
        "a query after WITH needs a table source".into(),
        statement,
    ));
}

/// A window frame (ROWS/RANGE) needs ORDER BY in the same OVER clause or a named base window.
/// Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set `context/window-*` and
/// window-no-order-*: Msg 10756 (Msg 102 for some ROWS shapes) without ORDER BY, also when ORDER BY
/// only sits in a nested query; accepted with ORDER BY and, at 160-170, with a base window.
fn window_frame_order(spec: Node, out: &mut Vec<Violation>) {
    let mut cursor = spec.walk();
    let children: Vec<Node> = spec.named_children(&mut cursor).collect();
    let Some(frame) = children.iter().find(|n| n.kind() == "window_frame") else {
        return;
    };
    if spec.child_by_field_name("base_window").is_none()
        && !children.iter().any(|n| n.kind() == "order_by")
    {
        out.push(violation(
            "window-frame-order",
            "a window frame needs ORDER BY".into(),
            *frame,
        ));
    }
}

/// MERGE must end with `;`: the next token after it is a semicolon, wherever the statement sits
/// (batch, block, IF or ELSE branch, procedure body). Evidence: SQL Server 2025 LocalDB 17.0.1000.7,
/// levels 100-170, oracle sets `context/merge-*` and `semicolon/merge-*`: Msg 10713 otherwise; a
/// comment before the semicolon is fine. A MERGE used as a table source (`FROM (MERGE ... OUTPUT
/// ...) AS x`) is not a statement and is not checked (accepted; fixture merge_as_from_source.sql).
fn merge_terminator(merge: Node, out: &mut Vec<Violation>) {
    if merge.parent().map(|p| p.kind()) != Some("statement") {
        return;
    }
    let mut n = merge;
    let next = loop {
        match n.next_sibling() {
            Some(s) if s.is_extra() => n = s,
            Some(s) => break Some(s),
            None => match n.parent() {
                Some(p) => n = p,
                None => break None,
            },
        }
    };
    // The token itself: the next node may start with it (`ELSE` clause after `IF ... MERGE ...;`).
    let first_token = next.map(|mut n| {
        while let Some(c) = n.child(0) {
            n = c;
        }
        n
    });
    if first_token.map(|n| n.kind()) != Some(";") {
        out.push(violation(
            "merge-terminator",
            "MERGE must end with ';'".into(),
            merge,
        ));
    }
}

/// Table hint names SQL Server knows (`hint-rules/one-*`; FASTFIRSTROW and other names are Msg 321).
const TABLE_HINTS: &[&str] = &[
    "NOLOCK",
    "READUNCOMMITTED",
    "UPDLOCK",
    "REPEATABLEREAD",
    "SERIALIZABLE",
    "HOLDLOCK",
    "READCOMMITTED",
    "READCOMMITTEDLOCK",
    "READPAST",
    "ROWLOCK",
    "PAGLOCK",
    "TABLOCK",
    "TABLOCKX",
    "XLOCK",
    "NOWAIT",
    "SNAPSHOT",
    "NOEXPAND",
    "FORCESCAN",
    "FORCESEEK",
    "INDEX",
    "SPATIAL_WINDOW_MAX_CELLS",
    "KEEPIDENTITY",
    "KEEPDEFAULTS",
    "IGNORE_CONSTRAINTS",
    "IGNORE_TRIGGERS",
];

/// Hints of which at most one may appear in a list (Msg 1047; FORCESEEK with FORCESCAN, Msg 10746),
/// after READUNCOMMITTED -> NOLOCK and SERIALIZABLE -> HOLDLOCK. Every pair of the 28 hints in
/// `hint-rules/pair-*` conflicts exactly when both are in one of these groups.
const HINT_GROUPS: &[&[&str]] = &[
    &["PAGLOCK", "NOLOCK", "ROWLOCK", "TABLOCK", "TABLOCKX"],
    &[
        "HOLDLOCK",
        "NOLOCK",
        "READCOMMITTED",
        "READCOMMITTEDLOCK",
        "REPEATABLEREAD",
        "SNAPSHOT",
    ],
    &["NOLOCK", "UPDLOCK", "XLOCK"],
    &["FORCESEEK", "FORCESCAN"],
];

/// A table hint list, `WITH ( ... )` on a table or `OPTION (TABLE HINT (alias, ...))`. Evidence:
/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set `hint-rules` (every hint alone,
/// every pair, every hint on each DML target) and the hint-*, pair-*, table-hints/* cases:
/// - an unknown name (Msg 321), and conflicting hints (`HINT_GROUPS`);
/// - FORCESEEK with an index and columns next to INDEX or another FORCESEEK (Msg 10747/10748);
///   FORCESEEK on index 0 (Msg 10749); SPATIAL_WINDOW_MAX_CELLS twice (Msg 1017);
/// - on the target of UPDATE, DELETE, INSERT or MERGE (`WITH` only, not `OPTION`): NOLOCK or
///   READUNCOMMITTED (Msg 1065); INDEX, FORCESEEK, FORCESCAN except on MERGE (Msg 1069, 10724,
///   10745); READPAST and READCOMMITTEDLOCK on INSERT (Msg 4102, 4140).
fn table_hints(list: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = list.walk();
    let children: Vec<Node> = list.children(&mut cursor).collect();
    if list.kind() == "query_hint" && !children.iter().any(|n| n.kind() == "keyword_table") {
        return;
    }
    // After a rowset function (`OPENJSON(@j) WITH (ROWLOCK int)`) the list is a column schema that
    // parses like hints; the engine accepts it (`table-hints/hint-keyword-schema`).
    let after_function = list.prev_named_sibling().is_some_and(|r| {
        r.kind() == "relation" && r.named_child(0).is_some_and(|n| n.kind() == "invocation")
    });
    if after_function {
        return;
    }
    let target = dml_target(list);
    let mut seen: Vec<String> = Vec::new();
    let (mut index, mut seek, mut seek_with_columns, mut spatial) = (false, 0, 0, 0);
    for item in children.iter().filter(|n| n.kind() == "table_hint_item") {
        let Some(first) = item.named_child(0) else {
            continue;
        };
        let name = upper_text(first, source);
        if !TABLE_HINTS.contains(&name.as_str()) {
            out.push(violation(
                "table-hint",
                format!("unknown table hint {name}"),
                *item,
            ));
            continue;
        }
        let name = match name.as_str() {
            "READUNCOMMITTED" => "NOLOCK".to_string(),
            "SERIALIZABLE" => "HOLDLOCK".to_string(),
            _ => name,
        };
        let mut c = item.walk();
        let with_arguments = item.children(&mut c).any(|n| n.kind() == "(");
        match name.as_str() {
            "INDEX" => index = true,
            "SPATIAL_WINDOW_MAX_CELLS" => spatial += 1,
            "FORCESEEK" => {
                seek += 1;
                if with_arguments {
                    seek_with_columns += 1;
                    let zero = item.named_child(1).is_some_and(|n| {
                        n.kind() == "literal" && upper_text(n, source).bytes().all(|b| b == b'0')
                    });
                    if zero {
                        out.push(violation(
                            "table-hint",
                            "FORCESEEK cannot use index 0".into(),
                            *item,
                        ));
                    }
                }
            }
            _ => {}
        }
        if let Some(statement) = target {
            let refused = match name.as_str() {
                "NOLOCK" => true,
                "INDEX" | "FORCESEEK" | "FORCESCAN" => statement != "merge",
                "READPAST" | "READCOMMITTEDLOCK" => statement == "insert",
                _ => false,
            };
            if refused {
                out.push(violation(
                    "table-hint",
                    format!(
                        "{name} cannot be used on the target of {}",
                        statement.to_uppercase()
                    ),
                    *item,
                ));
            }
        }
        if !seen.contains(&name) {
            seen.push(name);
        }
    }
    let conflict = HINT_GROUPS
        .iter()
        .any(|group| seen.iter().filter(|n| group.contains(&n.as_str())).count() > 1);
    let detail = if conflict {
        "conflicting table hints"
    } else if seek_with_columns > 0 && (index || seek > 1) {
        "FORCESEEK with an index cannot be combined with INDEX or another FORCESEEK"
    } else if spatial > 1 {
        "SPATIAL_WINDOW_MAX_CELLS appears twice"
    } else {
        return;
    };
    out.push(violation("table-hint", detail.into(), list));
}

/// The statement whose write target carries this `WITH ( ... )` list, if any: the list is a direct
/// child of UPDATE, DELETE or INSERT, or of MERGE before USING (a hint on the USING source is not a
/// target hint).
fn dml_target(list: Node) -> Option<&'static str> {
    if list.kind() != "table_hint" {
        return None;
    }
    let statement = match list.parent()?.kind() {
        "update" => "update",
        "delete" => "delete",
        "insert" => "insert",
        "merge" => "merge",
        _ => return None,
    };
    let mut prev = list.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "keyword_using" {
            return None;
        }
        prev = p.prev_sibling();
    }
    Some(statement)
}

/// `OPTION (TABLE HINT (alias, ...), TABLE HINT (alias, ...))` names each alias once. Evidence: SQL
/// Server 2025 LocalDB 17.0.1000.7, levels 100-170: Msg 8720 for `hint-rules/option-same-alias-twice`;
/// two different aliases accepted (`hint-rules/option-two-lists`).
fn table_hint_aliases(option: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut seen = Vec::new();
    let mut cursor = option.walk();
    for hint in option
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "query_hint")
    {
        let mut c = hint.walk();
        let kids: Vec<Node> = hint.named_children(&mut c).collect();
        // keyword_table, the word HINT (an identifier), then the alias.
        if kids.first().map(|n| n.kind()) != Some("keyword_table") {
            continue;
        }
        let Some(alias) = kids.get(2).map(|n| upper_text(*n, source)) else {
            continue;
        };
        if seen.contains(&alias) {
            out.push(violation(
                "table-hint",
                format!("TABLE HINT names {alias} twice"),
                hint,
            ));
        }
        seen.push(alias);
    }
}

/// EXTERNAL option sets measured with LocalDB 17.0.1000.7 at levels 100-170:
/// ddl-external/external-* duplicates are Msg 102/1039; CREATE LANGUAGE requires CONTENT and
/// FILE_NAME (39123), CREATE LIBRARY requires CONTENT (319); both ALTER forms may omit CONTENT.
/// Missing model LOCATION, file-format FORMAT_TYPE and external-table DATA_SOURCE are accepted
/// by PARSEONLY, so they are deliberately not required here (ADR-0003).
fn external_options(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let kind = statement.kind();
    let language = kind.ends_with("_language");
    let library = kind.ends_with("_library");
    let mut cursor = statement.walk();
    if language {
        let children: Vec<Node> = statement
            .named_children(&mut cursor)
            .filter(|n| !n.is_extra())
            .collect();
        if children.iter().any(|n| n.kind() == "keyword_remove")
            && let Some(platform) = children.last()
            && !matches!(upper_text(*platform, source).as_str(), "WINDOWS" | "LINUX")
        {
            out.push(violation(
                "external-platform",
                "PLATFORM must be WINDOWS or LINUX".into(),
                *platform,
            ));
        }
    }
    let mut cursor = statement.walk();
    let containers: Vec<Node> = statement
        .named_children(&mut cursor)
        .filter(|n| matches!(n.kind(), "external_file_spec" | "with_options"))
        .collect();
    // ALTER DATA SOURCE/MODEL has its option list directly on the statement.
    let containers = if containers.is_empty()
        && matches!(kind, "alter_external_data_source" | "alter_external_model")
    {
        vec![statement]
    } else {
        containers
    };
    for container in containers {
        let file = container.kind() == "external_file_spec";
        let allowed: &[&str] = if file && language {
            &[
                "CONTENT",
                "FILE_NAME",
                "PLATFORM",
                "PARAMETERS",
                "ENVIRONMENT_VARIABLES",
            ]
        } else if file && library {
            &["CONTENT", "PLATFORM"]
        } else if library {
            &["LANGUAGE"]
        } else if kind.ends_with("_resource_pool") {
            &[
                "MAX_CPU_PERCENT",
                "MAX_MEMORY_PERCENT",
                "MAX_PROCESSES",
                "AFFINITY",
            ]
        } else if kind.ends_with("_data_source") {
            &[
                "LOCATION",
                "CREDENTIAL",
                "CONNECTION_OPTIONS",
                "PUSHDOWN",
                "RESOURCE_MANAGER_LOCATION",
                "TYPE",
                "DATABASE_NAME",
                "SHARD_MAP_NAME",
            ]
        } else {
            &[
                "LOCATION",
                "API_FORMAT",
                "MODEL_TYPE",
                "MODEL",
                "CREDENTIAL",
                "PARAMETERS",
                "LOCAL_RUNTIME_PATH",
            ]
        };
        let mut seen = Vec::new();
        let mut c = container.walk();
        for option in container
            .named_children(&mut c)
            .filter(|n| n.kind() == "option")
        {
            let Some(name) = option.child_by_field_name("name") else {
                continue;
            };
            let key = upper_text(name, source);
            if seen.contains(&key) {
                out.push(violation(
                    "external-option-duplicate",
                    format!("{key} appears more than once"),
                    option,
                ));
            } else {
                seen.push(key.clone());
            }
            if !allowed.contains(&key.as_str()) {
                out.push(violation(
                    "external-option",
                    format!("{key} is not valid in this EXTERNAL option set"),
                    option,
                ));
            }
            if key == "AFFINITY" {
                let mut ac = option.walk();
                let children: Vec<Node> = option
                    .named_children(&mut ac)
                    .filter(|n| !n.is_extra())
                    .collect();
                if let Some(scalar) = children
                    .iter()
                    .find(|n| n.kind() == "identifier" && n.id() != name.id())
                    && (upper_text(*scalar, source) != "AUTO"
                        || !children.iter().any(|n| n.kind() == "keyword_cpu"))
                {
                    out.push(violation(
                        "external-affinity",
                        "only AFFINITY CPU accepts AUTO; otherwise use a range list".into(),
                        *scalar,
                    ));
                }
            }
            let mut o = option.walk();
            if !option.children(&mut o).any(|n| n.kind() == "=") {
                out.push(violation(
                    "external-option-assignment",
                    format!("{key} requires '='"),
                    option,
                ));
            }
            if key == "PLATFORM"
                && let Some(value) = option.child_by_field_name("value")
                && !matches!(upper_text(value, source).as_str(), "WINDOWS" | "LINUX")
            {
                out.push(violation(
                    "external-platform",
                    "PLATFORM must be WINDOWS or LINUX".into(),
                    value,
                ));
            }
        }
        let required: &[&str] = if file && kind == "create_external_language" {
            &["CONTENT", "FILE_NAME"]
        } else if file && kind == "create_external_library" {
            &["CONTENT"]
        } else if library && !file {
            &["LANGUAGE"]
        } else {
            &[]
        };
        for key in required {
            if !seen.iter().any(|k| k == key) {
                out.push(violation(
                    "external-option-required",
                    format!("{kind} requires {key}"),
                    container,
                ));
            }
        }
    }
}
/// FULLTEXT options appear at most once; CHANGE_TRACKING names a mode and NO POPULATION needs
/// CHANGE_TRACKING OFF in CREATE. Evidence: LocalDB 17.0.1000.7, levels 100-170,
/// ddl-external/fulltext-{catalog-accent,index-change,index-stoplist,index-search}-duplicate,
/// index-change-unknown and index-no-population-alone: Msg 102; index-no-population-auto: Msg
/// 7663. CREATE contains both settings, so their conflict needs no catalog state (ADR-0003).
fn fulltext_options(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let mut seen = Vec::new();
    let mut tracking_off = false;
    let mut no_population = None;
    for clause in statement
        .named_children(&mut cursor)
        .filter(|n| matches!(n.kind(), "with_clause" | "with_options"))
    {
        let mut c = clause.walk();
        for option in clause
            .named_children(&mut c)
            .filter(|n| n.kind() == "option")
        {
            let Some(name) = option.child_by_field_name("name") else {
                continue;
            };
            let key = upper_text(name, source);
            if seen.contains(&key) {
                out.push(violation(
                    "fulltext-option-duplicate",
                    format!("{key} appears more than once in FULLTEXT options"),
                    option,
                ));
            } else {
                seen.push(key.clone());
            }
            if statement.kind() != "create_fulltext_index" {
                continue;
            }
            let value = option
                .child_by_field_name("value")
                .map(|n| upper_text(n, source))
                .unwrap_or_default();
            if key == "CHANGE_TRACKING" {
                tracking_off |= value == "OFF";
                if !matches!(value.as_str(), "AUTO" | "MANUAL" | "OFF") {
                    out.push(violation(
                        "fulltext-change-tracking",
                        "CHANGE_TRACKING needs AUTO, MANUAL or OFF".into(),
                        option,
                    ));
                }
            } else if key == "NO" && value == "POPULATION" {
                no_population = Some(option);
            }
        }
    }
    if let Some(option) = no_population
        && !tracking_off
    {
        out.push(violation(
            "fulltext-no-population",
            "CREATE FULLTEXT INDEX NO POPULATION needs CHANGE_TRACKING OFF".into(),
            option,
        ));
    }
}
/// Endpoint option sets and enum values checked by the engine while parsing (ADR-0003).
/// Evidence: LocalDB 17.0.1000.7, levels 100-170, ddl-external/endpoint-* probes:
/// duplicate options Msg 7816, CREATE's missing LISTENER_PORT/ROLE Msg 7801, unknown options,
/// protocol/algorithm/role Msg 102, MESSAGE_FORWARDING = ON Msg 156, IPv4 octets Msg 7810,
/// CREATE STATE = DISABLED Msg 7871. TSQL ENCRYPTION = REQUIRED
/// and SERVICE_BROKER ENCRYPTION = STRICT are accepted, so no payload-specific mode check.
fn endpoint_options(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let creating = statement.kind() == "create_endpoint";
    let mut cursor = statement.walk();
    for clause in statement.named_children(&mut cursor) {
        let kind = clause.kind();
        if creating
            && kind == "endpoint_state"
            && clause
                .child_by_field_name("value")
                .map(|n| upper_text(n, source))
                == Some("DISABLED".into())
        {
            out.push(violation(
                "endpoint-create-state",
                "CREATE ENDPOINT cannot specify STATE = DISABLED".into(),
                clause,
            ));
        }
        if !matches!(kind, "endpoint_transport" | "endpoint_payload") {
            continue;
        }
        let payload = clause
            .child_by_field_name("type")
            .map(|n| upper_text(n, source))
            .unwrap_or_default();
        let allowed: &[&str] = match (kind, payload.as_str()) {
            ("endpoint_transport", _) => &["LISTENER_PORT", "LISTENER_IP"],
            (_, "TSQL") => &["ENCRYPTION"],
            (_, "SERVICE_BROKER") => &[
                "AUTHENTICATION",
                "ENCRYPTION",
                "MESSAGE_FORWARDING",
                "MESSAGE_FORWARD_SIZE",
            ],
            (_, "DATABASE_MIRRORING") => &["AUTHENTICATION", "ENCRYPTION", "ROLE"],
            _ => &[],
        };
        let mut seen = std::collections::HashSet::new();
        let mut child_cursor = clause.walk();
        for option in clause
            .named_children(&mut child_cursor)
            .filter(|n| n.kind() == "option")
        {
            let Some(name_node) = option.child_by_field_name("name") else {
                continue;
            };
            let name = upper_text(name_node, source);
            if !allowed.contains(&name.as_str()) {
                out.push(violation(
                    "endpoint-option",
                    format!("{name} is not valid in this endpoint option set"),
                    option,
                ));
                continue;
            }
            if !seen.insert(name.clone()) {
                out.push(violation(
                    "endpoint-duplicate",
                    format!("{name} occurs more than once"),
                    option,
                ));
            }
            if let Some(protocol) = option.child_by_field_name("protocol")
                && !matches!(
                    upper_text(protocol, source).as_str(),
                    "NTLM" | "KERBEROS" | "NEGOTIATE"
                )
            {
                out.push(violation(
                    "endpoint-authentication",
                    "WINDOWS protocol must be NTLM, KERBEROS or NEGOTIATE".into(),
                    protocol,
                ));
            }
            if let Some(value) = option.child_by_field_name("value") {
                let text = upper_text(value, source);
                let valid = match name.as_str() {
                    "ROLE" => matches!(text.as_str(), "WITNESS" | "PARTNER" | "ALL"),
                    "ENCRYPTION" => matches!(
                        text.as_str(),
                        "DISABLED" | "SUPPORTED" | "REQUIRED" | "NEGOTIATED" | "STRICT"
                    ),
                    "MESSAGE_FORWARDING" => matches!(text.as_str(), "ENABLED" | "DISABLED"),
                    "LISTENER_IP"
                        if !text.starts_with("'") && !text.starts_with("N'") && text != "ALL" =>
                    {
                        text.split('.').count() == 4
                            && text.split('.').all(|part| part.parse::<u8>().is_ok())
                    }
                    _ => true,
                };
                if !valid {
                    out.push(violation(
                        "endpoint-value",
                        format!("invalid {name} value"),
                        value,
                    ));
                }
            }
            let mut algorithm_cursor = option.walk();
            let algorithms: Vec<Node> = option
                .children_by_field_name("algorithm", &mut algorithm_cursor)
                .collect();
            let mut seen_algorithms = std::collections::HashSet::new();
            for algorithm in &algorithms {
                let text = upper_text(*algorithm, source);
                if !matches!(text.as_str(), "AES" | "RC4") || !seen_algorithms.insert(text) {
                    out.push(violation(
                        "endpoint-algorithm",
                        "ALGORITHM accepts AES, RC4 or each once in either order".into(),
                        *algorithm,
                    ));
                }
            }
            if !algorithms.is_empty()
                && option
                    .child_by_field_name("value")
                    .map(|n| upper_text(n, source))
                    == Some("DISABLED".into())
            {
                out.push(violation(
                    "endpoint-algorithm",
                    "DISABLED cannot specify ALGORITHM".into(),
                    option,
                ));
            }
        }
        let required = if !creating {
            None
        } else if kind == "endpoint_transport" {
            Some("LISTENER_PORT")
        } else if payload == "DATABASE_MIRRORING" {
            Some("ROLE")
        } else {
            None
        };
        if let Some(name) = required
            && !seen.contains(name)
        {
            out.push(violation(
                "endpoint-required",
                format!("CREATE ENDPOINT requires {name}"),
                clause,
            ));
        }
    }
}

fn upper_text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or_default()
        .to_ascii_uppercase()
}

fn violation(rule: &'static str, detail: String, node: Node) -> Violation {
    let at = node.start_position();
    Violation {
        rule,
        detail,
        row: at.row,
        column: at.column,
    }
}

/// CREATE/ALTER QUEUE ... WITH ACTIVATION (...): each setting at most once; CREATE also needs
/// PROCEDURE_NAME, MAX_QUEUE_READERS and EXECUTE AS. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, Msg 102 for oracle cases queue-duplicate-create/alter and
/// queue-missing-48/49/50, acceptance of queue-missing-51.
fn queue_activation(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let with_clauses = statement
        .children(&mut cursor)
        .filter(|n| n.kind() == "with_clause");
    for with_clause in with_clauses.collect::<Vec<_>>() {
        let mut c = with_clause.walk();
        for activation in with_clause.children(&mut c).filter(|n| {
            n.kind() == "option"
                && n.child_by_field_name("name")
                    .map(|name| upper_text(name, source))
                    == Some("ACTIVATION".into())
        }) {
            let mut seen: Vec<String> = Vec::new();
            let mut drop = false;
            let mut a = activation.walk();
            for setting in activation.named_children(&mut a) {
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
                if seen.contains(&key) {
                    out.push(violation(
                        "queue-activation-duplicate",
                        format!("{key} appears more than once in ACTIVATION"),
                        setting,
                    ));
                } else {
                    seen.push(key);
                }
            }
            if statement.kind() == "create_queue" && !drop {
                for required in ["PROCEDURE_NAME", "MAX_QUEUE_READERS", "EXECUTE AS"] {
                    if !seen.iter().any(|k| k == required) {
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
fn database_file_options(file: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut parent = file.parent();
    let mut required_filename = false;
    while let Some(statement) = parent {
        if matches!(statement.kind(), "create_database" | "alter_database") {
            let mut c = statement.walk();
            let kinds: Vec<&str> = statement.named_children(&mut c).map(|n| n.kind()).collect();
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
        parent = statement.parent();
    }
    option_set(
        file,
        source,
        "database-file-required",
        "database-file-duplicate",
        if required_filename {
            &["FILENAME"]
        } else {
            &[]
        },
        // ddl-database/extra-file-duplicates/case-001..006: Msg 153, levels 100-170.
        &[
            "NAME",
            "SIZE",
            "OFFLINE",
            "FILENAME",
            "MAXSIZE",
            "FILEGROWTH",
        ],
        out,
    );
}

/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170: ddl-database/ag/negative-suspected-003,
/// 004 and 005 reject ADD REPLICA without ENDPOINT_URL, AVAILABILITY_MODE and FAILOVER_MODE
/// with Msg 35236, 41198 and 41197 respectively. Distributed JOIN has different required options.
/// CREATE's required options: ddl-database/edge/3..5 (normal REPLICA, not distributed groups).
/// ADD duplicates: ddl-database/extra-ag-option-sets/case-003..005, Msg 1039.
fn availability_replica_options(replica: Node, source: &[u8], out: &mut Vec<Violation>) {
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
    let mut c = statement.walk();
    if (statement.kind() == "create_availability_group" && has_child(statement, "keyword_replica"))
        || statement
            .named_children(&mut c)
            .any(|n| n.kind() == "keyword_add")
    {
        option_set(
            replica,
            source,
            "availability-replica-required",
            "availability-replica-duplicate",
            &["ENDPOINT_URL", "AVAILABILITY_MODE", "FAILOVER_MODE"],
            &["ENDPOINT_URL", "AVAILABILITY_MODE", "FAILOVER_MODE"],
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
            "SECONDARY_ROLE" => &["ALLOW_CONNECTIONS"],
            "PRIMARY_ROLE" => &["READ_ONLY_ROUTING_LIST"],
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

/// Only direct options count: a nested option cannot satisfy its container's requirements.
/// BUFFER POOL evidence: ddl-database/server/negative-suspected-001/002, Msg 102 at levels 100-170
/// on SQL Server 2025 LocalDB 17.0.1000.7; server/positive-011 accepts both FILENAME and SIZE.
fn option_set(
    container: Node,
    source: &[u8],
    required_rule: &'static str,
    duplicate_rule: &'static str,
    required: &[&str],
    unique: &[&str],
    out: &mut Vec<Violation>,
) {
    let mut seen = Vec::new();
    let mut c = container.walk();
    for option in container
        .named_children(&mut c)
        .filter(|n| n.kind() == "option")
    {
        let Some(name) = option.child_by_field_name("name") else {
            continue;
        };
        let key = upper_text(name, source);
        if unique.contains(&key.as_str()) && seen.contains(&key) {
            out.push(violation(
                duplicate_rule,
                format!("{key} appears more than once in {}", container.kind()),
                option,
            ));
        }
        seen.push(key);
    }
    for key in required {
        if !seen.iter().any(|name| name.as_str() == *key) {
            out.push(violation(
                required_rule,
                format!("{} needs {key}", container.kind()),
                container,
            ));
        }
    }
}

fn direct_options(node: Node) -> Vec<Node> {
    let mut c = node.walk();
    node.named_children(&mut c)
        .filter(|n| n.kind() == "option")
        .collect()
}

fn has_child(node: Node, kind: &str) -> bool {
    let mut c = node.walk();
    node.children(&mut c).any(|n| n.kind() == kind)
}

fn next_option_unit(value: Node) -> Option<Node> {
    let mut next = value.next_named_sibling();
    while next.is_some_and(|n| n.is_extra()) {
        next = next.and_then(|n| n.next_named_sibling());
    }
    next
}

fn option_name(option: Node, source: &[u8]) -> String {
    option
        .child_by_field_name("name")
        .map(|n| upper_text(n, source))
        .unwrap_or_default()
}

/// ddl-database/extra-ag-option-sets/case-001/002 reject two SET options, even distinct (Msg 102).
fn availability_set_options(statement: Node, out: &mut Vec<Violation>) {
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
fn nested_database_options(option: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut parent = Some(option);
    let mut query_store = false;
    let mut database = false;
    while let Some(n) = parent {
        query_store |= n.kind() == "option" && option_name(n, source) == "QUERY_STORE";
        if matches!(n.kind(), "create_database" | "alter_database") {
            database = true;
            break;
        }
        parent = n.parent();
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
                    .and_then(next_option_unit)
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
        &[
            "OPERATION_MODE",
            "CLEANUP_POLICY",
            "STALE_QUERY_THRESHOLD_DAYS",
            "EXECUTION_COUNT",
        ]
    } else {
        match option_name(option, source).as_str() {
            "FILESTREAM" => &["DIRECTORY_NAME"],
            "CHANGE_TRACKING" => &["AUTO_CLEANUP"],
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
fn audit_clause_count(statement: Node, out: &mut Vec<Violation>) {
    let mut seen = Vec::new();
    let mut c = statement.walk();
    for child in statement.named_children(&mut c) {
        if !matches!(child.kind(), "keyword_to" | "with_options") {
            continue;
        }
        if seen.contains(&child.kind()) {
            out.push(violation(
                "server-audit-duplicate-clause",
                format!("{} appears more than once", child.kind()),
                child,
            ));
        }
        seen.push(child.kind());
    }
}

/// ddl-database/extra-buffer-pool/case-001..006: Msg 102 for duplicates, bare keys, reverse order
/// and an identifier FILENAME. The accepted server/positive-011 has FILENAME then numeric SIZE.
/// ddl-database/edge/9..11 reject missing unit, quoted size and TB, respectively (Msg 102).
fn buffer_pool_options(options: Node, source: &[u8], out: &mut Vec<Violation>) {
    option_set(
        options,
        source,
        "server-buffer-pool-required",
        "server-buffer-pool-duplicate",
        &["FILENAME", "SIZE"],
        &["FILENAME", "SIZE"],
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
                        && next_option_unit(v).is_some_and(|unit| {
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
fn encryption_algorithm(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = statement.walk();
    for clause in statement
        .named_children(&mut c)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn rules(sql: &str) -> Vec<&'static str> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
        let tree = p.parse(sql, None).unwrap();
        check(&tree, sql.as_bytes())
            .into_iter()
            .map(|v| v.rule)
            .collect()
    }

    const FULL: &str = "PROCEDURE_NAME = dbo.p, MAX_QUEUE_READERS = 1, EXECUTE AS OWNER";

    #[test]
    fn query_capture_policy_has_known_keys_and_hour_or_day_units() {
        for unit in ["HOURS", "DAYS"] {
            let sql = format!(
                "ALTER DATABASE d SET QUERY_STORE = ON (QUERY_CAPTURE_POLICY = (STALE_CAPTURE_POLICY_THRESHOLD = 5 {unit}));"
            );
            assert!(rules(&sql).is_empty(), "{sql}");
        }
        for option in ["STALE_CAPTURE_POLICY_THRESHOLD = 5 MINUTES", "MYSTERY = 7"] {
            let sql = format!(
                "ALTER DATABASE d SET QUERY_STORE = ON (QUERY_CAPTURE_POLICY = ({option}));"
            );
            assert_eq!(rules(&sql), ["query-capture-policy-option"]);
        }
    }

    #[test]
    fn distributed_groups_use_listener_url_not_listener() {
        let prefix = "ALTER AVAILABILITY GROUP a MODIFY AVAILABILITY GROUP ON 'east' WITH";
        assert_eq!(
            rules(&format!("{prefix} (LISTENER = 'TCP://east:5022');")),
            ["distributed-availability-option"]
        );
        assert!(rules(&format!("{prefix} (LISTENER_URL = 'TCP://east:5022');")).is_empty());
    }

    #[test]
    fn remaining_file_attributes_cannot_repeat() {
        for repeated in [
            "FILENAME = 'C:\\b.ndf'",
            "MAXSIZE = 8 MB",
            "FILEGROWTH = 2 MB",
        ] {
            let sql = format!(
                "ALTER DATABASE d ADD FILE (FILENAME = 'C:\\a.ndf', MAXSIZE = 4 MB, FILEGROWTH = 1 MB, {repeated});"
            );
            assert_eq!(rules(&sql), ["database-file-duplicate"], "{sql}");
        }
    }

    #[test]
    fn availability_set_and_modify_have_one_option_per_level() {
        assert_eq!(
            rules(
                "ALTER AVAILABILITY GROUP a SET (DB_FAILOVER = ON, HEALTH_CHECK_TIMEOUT = 30000);"
            ),
            ["availability-set-single-option"]
        );
        assert_eq!(
            rules(
                "ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'n' WITH (SESSION_TIMEOUT = 20, BACKUP_PRIORITY = 40);"
            ),
            ["availability-modify-single-option"]
        );
        assert_eq!(
            rules(
                "ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'n' WITH (PRIMARY_ROLE (ALLOW_CONNECTIONS = ALL, READ_ONLY_ROUTING_LIST = ('b')));"
            ),
            ["availability-modify-role-single-option"]
        );
        assert!(rules("ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'n' WITH (PRIMARY_ROLE (ALLOW_CONNECTIONS = ALL));").is_empty());
        assert!(rules("ALTER AVAILABILITY GROUP a SET (DB_FAILOVER = ON);").is_empty());
    }

    #[test]
    fn availability_required_and_duplicate_options_are_local_to_each_replica() {
        let full = "ENDPOINT_URL = 'TCP://n:5022', AVAILABILITY_MODE = SYNCHRONOUS_COMMIT, FAILOVER_MODE = MANUAL";
        assert_eq!(
            rules(&format!(
                "ALTER AVAILABILITY GROUP a ADD REPLICA ON 'n' WITH ({full}, ENDPOINT_URL = 'TCP://n:5023');"
            )),
            ["availability-replica-duplicate"]
        );
        assert_eq!(
            rules(
                "CREATE AVAILABILITY GROUP a FOR DATABASE d REPLICA ON 'n' WITH (AVAILABILITY_MODE = SYNCHRONOUS_COMMIT, FAILOVER_MODE = MANUAL);"
            ),
            ["availability-replica-required"]
        );
        for role in [
            "PRIMARY_ROLE (READ_ONLY_ROUTING_LIST = ('b'), READ_ONLY_ROUTING_LIST = NONE)",
            "SECONDARY_ROLE (ALLOW_CONNECTIONS = ALL, ALLOW_CONNECTIONS = READ_ONLY)",
        ] {
            let sql =
                format!("ALTER AVAILABILITY GROUP a ADD REPLICA ON 'n' WITH ({full}, {role});");
            assert_eq!(rules(&sql), ["availability-role-duplicate"], "{sql}");
        }
    }

    #[test]
    fn database_nested_duplicates_do_not_restrict_top_level_repeats() {
        for option in [
            "QUERY_STORE = ON (OPERATION_MODE = READ_WRITE, OPERATION_MODE = READ_ONLY)",
            "QUERY_STORE = ON (CLEANUP_POLICY = (STALE_QUERY_THRESHOLD_DAYS = 4, STALE_QUERY_THRESHOLD_DAYS = 5))",
            "QUERY_STORE = ON (QUERY_CAPTURE_POLICY = (EXECUTION_COUNT = 4, EXECUTION_COUNT = 5))",
            "QUERY_STORE = ON (CLEANUP_POLICY = (STALE_QUERY_THRESHOLD_DAYS = 4), CLEANUP_POLICY = (STALE_QUERY_THRESHOLD_DAYS = 5))",
            "FILESTREAM (DIRECTORY_NAME = 'a', DIRECTORY_NAME = 'b')",
            "CHANGE_TRACKING = ON (AUTO_CLEANUP = ON, AUTO_CLEANUP = OFF)",
        ] {
            let sql = format!("ALTER DATABASE d SET {option};");
            assert_eq!(rules(&sql), ["database-option-duplicate"], "{sql}");
        }
        assert!(rules("ALTER DATABASE d SET AUTO_CLOSE ON, AUTO_CLOSE OFF, RECOVERY SIMPLE, RECOVERY FULL;").is_empty());
    }

    #[test]
    fn audit_clauses_cannot_repeat_but_state_within_a_clause_can() {
        for bad in [
            "ALTER SERVER AUDIT a TO SECURITY_LOG TO APPLICATION_LOG;",
            "ALTER SERVER AUDIT a WITH (STATE = OFF) WITH (QUEUE_DELAY = 900);",
        ] {
            assert_eq!(rules(bad), ["server-audit-duplicate-clause"], "{bad}");
        }
        assert!(rules("ALTER SERVER AUDIT a WITH (STATE = OFF, STATE = ON);").is_empty());
    }

    #[test]
    fn buffer_pool_checks_order_duplicates_and_assigned_values() {
        assert_eq!(
            rules(
                "ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON (SIZE = 12 GB, FILENAME = 'C:\\a.bpe');"
            ),
            ["server-buffer-pool-order"]
        );
        assert_eq!(
            rules(
                "ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON (FILENAME = 'C:\\a.bpe', SIZE = 12 GB, SIZE = 16 GB);"
            ),
            ["server-buffer-pool-duplicate"]
        );
        for options in [
            "FILENAME, SIZE = 12 GB",
            "FILENAME = path_name, SIZE = 12 GB",
            "FILENAME = 'C:\\a.bpe', SIZE",
            "FILENAME = 'C:\\a.bpe', SIZE = 12",
            "FILENAME = 'C:\\a.bpe', SIZE = '12' GB",
            "FILENAME = 'C:\\a.bpe', SIZE = 12 TB",
        ] {
            let sql =
                format!("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON ({options});");
            assert_eq!(rules(&sql), ["server-buffer-pool-value"], "{sql}");
        }
    }

    #[test]
    fn encryption_has_one_assigned_known_algorithm() {
        for options in [
            "ALGORITHM",
            "MYSTERY = AES_256",
            "ALGORITHM = MYSTERY",
            "ALGORITHM = AES_128, ALGORITHM = AES_256",
        ] {
            for sql in [
                format!(
                    "CREATE DATABASE ENCRYPTION KEY WITH {options} ENCRYPTION BY SERVER CERTIFICATE c;"
                ),
                format!("ALTER DATABASE ENCRYPTION KEY REGENERATE WITH {options};"),
            ] {
                assert_eq!(rules(&sql), ["database-encryption-algorithm"], "{sql}");
            }
        }
        for algorithm in ["AES_128", "AES_192", "AES_256", "TRIPLE_DES_3KEY"] {
            assert!(rules(&format!("CREATE DATABASE ENCRYPTION KEY WITH ALGORITHM = {algorithm} ENCRYPTION BY SERVER CERTIFICATE c;")).is_empty());
        }
        assert!(
            rules("ALTER DATABASE ENCRYPTION KEY ENCRYPTION BY SERVER CERTIFICATE c;").is_empty()
        );
    }

    #[test]
    fn database_file_requirements_follow_the_measured_operation() {
        for ok in [
            "ALTER DATABASE d ADD FILE (FILENAME = 'C:\\probe.ndf');",
            "ALTER DATABASE d MODIFY FILE (SIZE = 8 MB);",
            "CREATE DATABASE d ON (FILENAME = 'C:\\probe.mdf') FOR ATTACH;",
            "CREATE DATABASE d ON (NAME = f, FILENAME = 'C:\\probe.mdf');",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "ALTER DATABASE d ADD FILE (NAME = f);",
            "CREATE DATABASE d ON (NAME = f);",
        ] {
            assert_eq!(rules(bad), ["database-file-required"], "{bad}");
        }
    }

    #[test]
    fn measured_file_duplicates_are_rejected_case_insensitively() {
        for bad in [
            "ALTER DATABASE d ADD FILE (NAME = f, name = g, FILENAME = 'C:\\probe.ndf');",
            "ALTER DATABASE d ADD FILE (FILENAME = 'C:\\probe.ndf', SIZE = 1 MB, size = 2 MB);",
            "ALTER DATABASE d MODIFY FILE (NAME = f, OFFLINE, offline);",
        ] {
            assert_eq!(rules(bad), ["database-file-duplicate"], "{bad}");
        }
        assert!(rules("ALTER DATABASE d ADD FILE (NAME = f, FILENAME = 'C:\\a.ndf'), (NAME = g, FILENAME = 'C:\\b.ndf');").is_empty());
    }

    #[test]
    fn add_replica_requires_each_measured_option_but_modify_does_not() {
        let full = [
            "ENDPOINT_URL = 'TCP://probe:5022'",
            "AVAILABILITY_MODE = SYNCHRONOUS_COMMIT",
            "FAILOVER_MODE = MANUAL",
        ];
        for missing in 0..full.len() {
            let options = full
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != missing)
                .map(|(_, s)| *s)
                .collect::<Vec<_>>()
                .join(", ");
            let sql =
                format!("ALTER AVAILABILITY GROUP a ADD REPLICA ON 'probe' WITH ({options});");
            assert_eq!(rules(&sql), ["availability-replica-required"], "{sql}");
        }
        assert!(
            rules(&format!(
                "ALTER AVAILABILITY GROUP a ADD REPLICA ON 'probe' WITH ({});",
                full.join(", ")
            ))
            .is_empty()
        );
        assert!(rules("ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'probe' WITH (FAILOVER_MODE = MANUAL);").is_empty());
    }

    #[test]
    fn buffer_pool_extension_requires_filename_and_size() {
        for options in ["FILENAME = 'C:\\probe.bpe'", "SIZE = 12 GB"] {
            let sql =
                format!("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON ({options});");
            assert_eq!(rules(&sql), ["server-buffer-pool-required"], "{sql}");
        }
        assert!(rules("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON (FILENAME = 'C:\\probe.bpe', SIZE = 12 GB);").is_empty());
        assert!(rules("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION OFF;").is_empty());
    }

    #[test]
    fn complete_activation_in_any_order_passes() {
        assert!(rules(&format!("CREATE QUEUE q WITH ACTIVATION ({FULL});")).is_empty());
        let reordered = "EXECUTE AS SELF, STATUS = ON, MAX_QUEUE_READERS = 2, PROCEDURE_NAME = p";
        assert!(rules(&format!("CREATE QUEUE q WITH ACTIVATION ({reordered});")).is_empty());
    }

    #[test]
    fn create_needs_the_three_required_settings() {
        let missing = "CREATE QUEUE q WITH ACTIVATION (MAX_QUEUE_READERS = 1, EXECUTE AS OWNER, STATUS = OFF);";
        assert_eq!(rules(missing), ["queue-activation-required"]);
        assert_eq!(
            rules("CREATE QUEUE q WITH ACTIVATION (STATUS = ON);").len(),
            3
        );
    }

    #[test]
    fn alter_may_change_one_setting_or_drop() {
        assert!(rules("ALTER QUEUE q WITH ACTIVATION (STATUS = OFF);").is_empty());
        assert!(rules("ALTER QUEUE q WITH ACTIVATION (DROP);").is_empty());
    }

    #[test]
    fn a_repeated_setting_is_a_violation_in_create_and_alter() {
        let dup = format!("{FULL}, PROCEDURE_NAME = dbo.p");
        assert_eq!(
            rules(&format!("CREATE QUEUE q WITH ACTIVATION ({dup});")),
            ["queue-activation-duplicate"]
        );
        assert_eq!(
            rules(&format!("ALTER QUEUE q WITH ACTIVATION ({dup});")),
            ["queue-activation-duplicate"]
        );
        let exec_twice = "ALTER QUEUE q WITH ACTIVATION (EXECUTE AS SELF, EXECUTE AS OWNER);";
        assert_eq!(rules(exec_twice), ["queue-activation-duplicate"]);
    }

    const CTE: &str = "WITH c AS (SELECT 1 AS n) SELECT n FROM c;";

    #[test]
    fn a_cte_directly_after_a_statement_needs_a_terminator() {
        for before in [
            "SELECT 1
",
            "BEGIN SELECT 1; END
",
            "IF 1=1 SELECT 1
",
            "WHILE 1=0 SELECT 1
",
        ] {
            assert_eq!(
                rules(&format!("{before}{CTE}")),
                ["cte-terminator"],
                "{before}"
            );
        }
        assert_eq!(
            rules(&format!(
                "CREATE PROCEDURE dbo.p AS SELECT 1
{CTE}"
            )),
            ["cte-terminator"]
        );
    }

    #[test]
    fn a_terminated_leading_or_controlled_cte_passes() {
        for sql in [
            CTE.to_string(),
            format!("SELECT 1; {CTE}"),
            format!(";{CTE}"),
            format!("here: {CTE}"),
            format!("IF 1=1 {CTE}"),
            format!("WHILE 1=0 {CTE}"),
            format!("BEGIN {CTE} END;"),
            format!("BEGIN SELECT 1; END; {CTE}"),
            format!("CREATE PROCEDURE dbo.p AS {CTE}"),
        ] {
            assert!(rules(&sql).is_empty(), "{sql}");
        }
    }

    #[test]
    fn exec_output_parameters_need_at() {
        let q = "N'SELECT ?'";
        assert!(rules(&format!("DECLARE @v int; EXEC({q}, @v OUTPUT) AT [lnk];")).is_empty());
        assert!(rules(&format!("DECLARE @v int; EXEC({q}, @v OUT) AT lnk;")).is_empty());
        assert_eq!(
            rules(&format!("DECLARE @v int; EXEC({q}, @v OUTPUT);")),
            ["exec-output-requires-at"]
        );
        assert!(rules("DECLARE @v int; EXEC dbo.p @v OUTPUT;").is_empty());
    }

    #[test]
    fn output_marks_only_a_local_variable() {
        for ok in [
            "EXEC dbo.p @v OUTPUT;",
            "EXEC dbo.p @v OUT;",
            "EXEC dbo.p @a = @v OUTPUT;",
            "EXEC(N'x', @v OUTPUT) AT l;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "EXEC dbo.p 1 OUTPUT;",
            "EXEC dbo.p 'a' OUTPUT;",
            "EXEC dbo.p DEFAULT OUTPUT;",
            "EXEC dbo.p @@ROWCOUNT OUTPUT;",
            "EXEC dbo.p x OUTPUT;",
            "EXEC dbo.p @a = 1 OUTPUT;",
            "EXEC(N'x', 1 OUTPUT) AT l;",
            "EXEC(N'x', @@ROWCOUNT OUTPUT) AT l;",
        ] {
            assert_eq!(rules(bad), ["exec-output-variable"], "{bad}");
        }
    }

    #[test]
    fn column_lists_allow_only_some_permissions() {
        for ok in [
            "GRANT SELECT ON dbo.t (a) TO public;",
            "GRANT SELECT (a), INSERT ON dbo.t TO public;",
            "DENY UNMASK ON dbo.t (a) TO public;",
            "GRANT ALL PRIVILEGES ON dbo.t (a) TO public;",
            "GRANT DELETE ON dbo.t TO public;",
            "REVOKE GRANT OPTION FOR SELECT ON dbo.t (a) FROM public CASCADE;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "GRANT INSERT ON dbo.t (a) TO public;",
            "GRANT SELECT, INSERT (a) ON dbo.t TO public;",
            "REVOKE DELETE ON OBJECT::dbo.t (a) FROM public;",
            "DENY VIEW DEFINITION (a) ON dbo.t TO public;",
        ] {
            assert_eq!(rules(bad), ["column-permission"], "{bad}");
        }
    }

    #[test]
    fn trigger_events_match_the_scope() {
        for ok in [
            "CREATE TRIGGER tr ON DATABASE FOR CREATE_TABLE AS RETURN;",
            "CREATE TRIGGER tr ON ALL SERVER FOR CREATE_LOGIN AS RETURN;",
            "CREATE TRIGGER dbo.tr ON dbo.t WITH EXECUTE AS OWNER FOR INSERT, UPDATE AS RETURN;",
            "CREATE TRIGGER tr ON DATABASE WITH EXECUTE AS SELF FOR DDL_TABLE_EVENTS AS RETURN;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "CREATE TRIGGER dbo.tr ON DATABASE FOR CREATE_TABLE AS RETURN;",
            "ALTER TRIGGER [dbo].[tr] ON ALL SERVER FOR CREATE_TABLE AS RETURN;",
            "CREATE TRIGGER tr ON DATABASE FOR INSERT AS RETURN;",
            "CREATE TRIGGER tr ON dbo.t FOR CREATE_TABLE AS RETURN;",
            "CREATE TRIGGER tr ON DATABASE FOR DDL_SERVER_LEVEL_EVENTS AS RETURN;",
            "CREATE TRIGGER tr ON ALL SERVER FOR ALTER_SERVER_CONFIGURATION AS RETURN;",
            "CREATE OR ALTER TRIGGER tr ON DATABASE WITH EXECUTE AS OWNER FOR CREATE_TABLE AS RETURN;",
        ] {
            assert_eq!(rules(bad), ["trigger-scope"], "{bad}");
        }
    }

    #[test]
    fn fixed_arity_builtins_count_arguments() {
        for ok in [
            "SELECT COUNT(*), COUNT_BIG(DISTINCT a), SUM(a), ISNULL(a, 0) FROM dbo.t;",
            "SELECT STRING_AGG(a, ',') FROM dbo.t;",
            "SELECT dbo.COUNT(a, b), dbo.SUM() FROM dbo.t;",
            "SELECT GROUPING_ID(a, b) FROM dbo.t GROUP BY a, b;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "SELECT COUNT();",
            "SELECT count(a, b) FROM dbo.t;",
            "SELECT MAX() FROM dbo.t;",
            "SELECT ISNULL(a) FROM dbo.t;",
            "SELECT COUNT() OVER () FROM dbo.t;",
        ] {
            assert_eq!(rules(bad), ["builtin-arity"], "{bad}");
        }
    }

    #[test]
    fn scalar_functions_return_a_value() {
        for ok in [
            "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN 1; END;",
            "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN (1); END;",
            "CREATE FUNCTION dbo.f() RETURNS @r TABLE (a int) AS BEGIN RETURN; END;",
            "CREATE FUNCTION dbo.f() RETURNS TABLE AS RETURN SELECT 1 AS a;",
            "CREATE PROCEDURE dbo.p AS BEGIN RETURN; END;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN; END;",
            "ALTER FUNCTION dbo.f() RETURNS int AS BEGIN IF 1=1 RETURN; RETURN 1; END;",
        ] {
            assert_eq!(rules(bad), ["scalar-function-return"], "{bad}");
        }
    }

    #[test]
    fn a_cte_query_needs_a_table_source() {
        for ok in [
            "WITH c AS (SELECT 1 AS a) SELECT a FROM c;",
            "WITH c AS (SELECT 1 AS a) SELECT (SELECT a FROM c);",
            "WITH c AS (SELECT 1 AS a) SELECT 1 WHERE EXISTS (SELECT 1 FROM c);",
            "WITH c AS (SELECT 1 AS a) SELECT 1 UNION SELECT 2;",
            "WITH c AS (SELECT 1 AS a) UPDATE dbo.t SET a = 1;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "WITH c AS (SELECT 1 AS a) SELECT 1;",
            "WITH c AS (SELECT 1 AS a), d AS (SELECT a FROM c) SELECT 1;",
        ] {
            assert_eq!(rules(bad), ["cte-table-source"], "{bad}");
        }
    }

    #[test]
    fn a_window_frame_needs_order_by() {
        for ok in [
            "SELECT MAX(a) OVER (ORDER BY a ROWS 1 PRECEDING) FROM dbo.t;",
            "SELECT MAX(a) OVER (PARTITION BY a) FROM dbo.t;",
            "SELECT MAX(a) OVER (w ROWS 1 PRECEDING) FROM dbo.t WINDOW w AS (ORDER BY a);",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "SELECT MAX(a) OVER (PARTITION BY a RANGE UNBOUNDED PRECEDING) FROM dbo.t;",
            "SELECT MAX(a) OVER (PARTITION BY (SELECT 1 ORDER BY (SELECT 1) OFFSET 0 ROWS) ROWS 1 PRECEDING) FROM dbo.t;",
        ] {
            assert_eq!(rules(bad), ["window-frame-order"], "{bad}");
        }
    }

    #[test]
    fn merge_ends_with_a_semicolon() {
        const M: &str = "MERGE dbo.t USING dbo.s ON 1=1 WHEN MATCHED THEN DELETE";
        for ok in [
            format!("{M};"),
            format!("BEGIN {M}; END;"),
            format!("IF 1=1 {M}; ELSE SELECT 1;"),
            format!("{M} /* c */ ;"),
            format!("{M} OUTPUT $action;"),
            format!("INSERT INTO dbo.u SELECT a FROM ({M} OUTPUT deleted.a) AS x (a);"),
        ] {
            assert!(rules(&ok).is_empty(), "{ok}");
        }
        for bad in [
            M.to_string(),
            format!("BEGIN {M} END;"),
            format!("IF 1=1 {M} ELSE SELECT 1;"),
            format!("CREATE PROCEDURE dbo.p AS {M}"),
        ] {
            assert_eq!(rules(&bad), ["merge-terminator"], "{bad}");
        }
    }

    #[test]
    fn table_hints_follow_the_engine_matrix() {
        for ok in [
            "SELECT a FROM dbo.t WITH (NOLOCK, READUNCOMMITTED, NOWAIT);",
            "SELECT a FROM dbo.t WITH (SERIALIZABLE HOLDLOCK ROWLOCK UPDLOCK);",
            "SELECT a FROM dbo.t WITH (nolock, index(ix));",
            "SELECT a FROM dbo.t WITH (FORCESEEK, INDEX(ix), INDEX(0));",
            "SELECT a FROM dbo.t WITH (FORCESEEK(1(a)));",
            "SELECT a FROM dbo.t WITH (NOLOCK) JOIN dbo.s WITH (UPDLOCK) ON 1 = 1;",
            "SELECT a FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK), TABLE HINT (y, UPDLOCK));",
            "UPDATE x SET a = 1 FROM dbo.t AS x WITH (NOLOCK);",
            "UPDATE x SET a = 1 FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK));",
            "MERGE dbo.t WITH (INDEX(ix), FORCESCAN) AS t USING dbo.s AS s ON 1 = 1 WHEN MATCHED THEN DELETE;",
            "INSERT dbo.t WITH (TABLOCK) SELECT a FROM dbo.s WITH (NOLOCK);",
            "UPDATE dbo.t WITH (READPAST) SET a = 1;",
            "DECLARE @j nvarchar(max); SELECT * FROM OPENJSON(@j) WITH (ROWLOCK int);",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "SELECT a FROM dbo.t WITH (NOLOCK, HOLDLOCK);",
            "SELECT a FROM dbo.t WITH (READUNCOMMITTED UPDLOCK);",
            "SELECT a FROM dbo.t WITH (ROWLOCK, PAGLOCK);",
            "SELECT a FROM dbo.t WITH (FORCESEEK, FORCESCAN);",
            "SELECT a FROM dbo.t WITH (FORCESEEK(ix(a)), INDEX(ix));",
            "SELECT a FROM dbo.t WITH (FORCESEEK(ix(a)), FORCESEEK);",
            "SELECT a FROM dbo.t WITH (FORCESEEK(00(a)));",
            "SELECT a FROM dbo.t WITH (SPATIAL_WINDOW_MAX_CELLS=8, SPATIAL_WINDOW_MAX_CELLS=8);",
            "SELECT a FROM dbo.t WITH (NOLOCKX);",
            "SELECT a FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK, UPDLOCK));",
            "SELECT a FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK), TABLE HINT (x, UPDLOCK));",
            "UPDATE dbo.t WITH (NOLOCK) SET a = 1;",
            "DELETE FROM dbo.t WITH (FORCESCAN);",
            "INSERT INTO dbo.t WITH (READPAST) (a) VALUES (1);",
            "MERGE dbo.t WITH (READUNCOMMITTED) AS t USING dbo.s AS s ON 1 = 1 WHEN MATCHED THEN DELETE;",
        ] {
            assert_eq!(rules(bad), ["table-hint"], "{bad}");
        }
    }

    #[test]
    fn other_statements_are_not_checked() {
        assert!(rules("SELECT 1 WHERE 1 = 1;").is_empty());
    }

    #[test]
    fn external_option_sets_match_the_engine() {
        for sql in [
            "CREATE EXTERNAL LANGUAGE Java FROM (FILE_NAME = 'java.so', CONTENT = 0x504B0304);",
            "ALTER EXTERNAL LANGUAGE Java SET (FILE_NAME = 'java.so');",
            "ALTER EXTERNAL LANGUAGE Java REMOVE PLATFORM LINUX;",
            "CREATE EXTERNAL MODEL emb WITH (API_FORMAT = 'OpenAI', MODEL_TYPE = EMBEDDINGS, MODEL = 'm');",
            "ALTER EXTERNAL MODEL emb SET (PARAMETERS = '{}');",
            "CREATE EXTERNAL DATA SOURCE ds WITH (CREDENTIAL = cred);",
            "ALTER EXTERNAL LIBRARY pkg SET (PLATFORM = LINUX) WITH (LANGUAGE = 'Python');",
            "ALTER EXTERNAL RESOURCE POOL ep WITH (AFFINITY NUMANODE = (0 TO 1));",
        ] {
            assert!(rules(sql).is_empty(), "{sql}");
        }
        for (sql, rule) in [
            (
                "ALTER EXTERNAL RESOURCE POOL ep WITH (AFFINITY NUMANODE = AUTO);",
                "external-affinity",
            ),
            (
                "ALTER EXTERNAL RESOURCE POOL ep WITH (AFFINITY CPU = UNKNOWN_VALUE);",
                "external-affinity",
            ),
            (
                "CREATE EXTERNAL LANGUAGE Java FROM (FILE_NAME = 'java.so');",
                "external-option-required",
            ),
            (
                "CREATE EXTERNAL LANGUAGE Java FROM (CONTENT = 0x504B0304);",
                "external-option-required",
            ),
            (
                "CREATE EXTERNAL LANGUAGE Java FROM (CONTENT = 0x504B0304, FILE_NAME = 'java.so', FILE_NAME = 'java2.so');",
                "external-option-duplicate",
            ),
            (
                "CREATE EXTERNAL LIBRARY pkg FROM (PLATFORM = LINUX) WITH (LANGUAGE = 'Python');",
                "external-option-required",
            ),
            (
                "CREATE EXTERNAL LIBRARY pkg FROM (CONTENT = 0x504B0304, CONTENT = 0x504B0304) WITH (LANGUAGE = 'Python');",
                "external-option-duplicate",
            ),
            (
                "ALTER EXTERNAL LANGUAGE Java REMOVE PLATFORM MACOS;",
                "external-platform",
            ),
            (
                "ALTER EXTERNAL DATA SOURCE ds SET LOCATION = 'abs://a', LOCATION = 'abs://b';",
                "external-option-duplicate",
            ),
            (
                "CREATE EXTERNAL DATA SOURCE ds WITH (LOCATION = 'abs://a', LOCATION = 'abs://b');",
                "external-option-duplicate",
            ),
            (
                "ALTER EXTERNAL DATA SOURCE ds SET UNKNOWN_OPTION = 1;",
                "external-option",
            ),
            (
                "ALTER EXTERNAL RESOURCE POOL ep WITH (MAX_CPU_PERCENT = 10, MAX_CPU_PERCENT = 20);",
                "external-option-duplicate",
            ),
            (
                "CREATE EXTERNAL RESOURCE POOL ep WITH (UNKNOWN_OPTION = 1);",
                "external-option",
            ),
            (
                "CREATE EXTERNAL MODEL emb WITH (MODEL = 'm', MODEL = 'm2');",
                "external-option-duplicate",
            ),
        ] {
            assert_eq!(rules(sql), [rule], "{sql}");
        }
    }
    #[test]
    fn fulltext_options_appear_at_most_once() {
        for options in [
            "CHANGE_TRACKING AUTO, CHANGE_TRACKING MANUAL",
            "STOPLIST SYSTEM, STOPLIST OFF",
            "SEARCH PROPERTY LIST = spl, SEARCH PROPERTY LIST = spl2",
        ] {
            assert_eq!(
                rules(&format!(
                    "CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH ({options});"
                )),
                ["fulltext-option-duplicate"],
                "{options}"
            );
        }
        assert_eq!(
            rules(
                "CREATE FULLTEXT CATALOG ftc WITH ACCENT_SENSITIVITY = ON, ACCENT_SENSITIVITY = OFF;"
            ),
            ["fulltext-option-duplicate"]
        );
        assert!(rules("CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH (CHANGE_TRACKING AUTO, STOPLIST SYSTEM, SEARCH PROPERTY LIST = spl);").is_empty());
    }

    #[test]
    fn fulltext_change_tracking_has_a_mode() {
        for mode in ["AUTO", "MANUAL", "OFF"] {
            for sql in [
                format!(
                    "CREATE FULLTEXT INDEX ON dbo.docs KEY INDEX pk WITH (CHANGE_TRACKING = {mode});"
                ),
                format!(
                    "CREATE FULLTEXT INDEX ON dbo.docs KEY INDEX pk WITH CHANGE_TRACKING {mode};"
                ),
            ] {
                assert!(rules(&sql).is_empty(), "{sql}");
            }
        }
        assert_eq!(
            rules(
                "CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH (CHANGE_TRACKING BOGUS);"
            ),
            ["fulltext-change-tracking"]
        );
    }

    #[test]
    fn fulltext_no_population_requires_tracking_off_in_create() {
        for options in ["NO POPULATION", "CHANGE_TRACKING AUTO, NO POPULATION"] {
            assert_eq!(
                rules(&format!(
                    "CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH ({options});"
                )),
                ["fulltext-no-population"],
                "{options}"
            );
        }
        assert!(rules("CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH (CHANGE_TRACKING OFF, NO POPULATION, STOPLIST sl);").is_empty());
        assert!(rules("CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH change_tracking /*mode*/ off, no /*skip*/ population;").is_empty());
        assert!(
            rules("ALTER FULLTEXT INDEX ON dbo.docs SET STOPLIST SYSTEM WITH NO POPULATION;")
                .is_empty()
        );
    }
    #[test]
    fn endpoint_sets_match_the_engine() {
        for ok in [
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = ALL) FOR TSQL ();",
            "CREATE ENDPOINT ep AS TCP (LISTENER_IP = ALL, LISTENER_PORT = 1434) FOR TSQL (ENCRYPTION = REQUIRED);",
            "ALTER ENDPOINT ep STATE = STOPPED;",
            "ALTER ENDPOINT ep AS TCP (LISTENER_IP = ALL);",
            "ALTER ENDPOINT ep FOR DATABASE_MIRRORING ();",
            "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ENCRYPTION = REQUIRED);",
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = STRICT);",
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = WINDOWS NTLM CERTIFICATE c, ENCRYPTION = REQUIRED ALGORITHM AES RC4);",
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = CERTIFICATE c WINDOWS NEGOTIATE, ENCRYPTION = SUPPORTED ALGORITHM RC4 AES);",
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (MESSAGE_FORWARD_SIZE = 10, MESSAGE_FORWARDING = ENABLED, ENCRYPTION = REQUIRED);",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for (bad, rule) in [
            (
                "ALTER ENDPOINT ep FOR TSQL (ENCRYPTION = BOGUS);",
                "endpoint-value",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = BOGUS);",
                "endpoint-value",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = (10)) FOR TSQL ();",
                "endpoint-value",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = (10.0)) FOR TSQL ();",
                "endpoint-value",
            ),
            (
                "CREATE ENDPOINT ep STATE = DISABLED AS TCP (LISTENER_PORT = 1436) FOR TSQL ();",
                "endpoint-create-state",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = (999.0.0.1)) FOR TSQL ();",
                "endpoint-value",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_IP = ALL) FOR TSQL ();",
                "endpoint-required",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 5022) FOR DATABASE_MIRRORING (ENCRYPTION = REQUIRED);",
                "endpoint-required",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_PORT = 1435) FOR TSQL ();",
                "endpoint-duplicate",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = ALL, LISTENER_IP = ALL) FOR TSQL ();",
                "endpoint-duplicate",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = REQUIRED, ENCRYPTION = SUPPORTED);",
                "endpoint-duplicate",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = WINDOWS, AUTHENTICATION = CERTIFICATE c);",
                "endpoint-duplicate",
            ),
            (
                "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ROLE = WITNESS, ROLE = ALL);",
                "endpoint-duplicate",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = WINDOWS BASIC);",
                "endpoint-authentication",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = REQUIRED ALGORITHM DES);",
                "endpoint-algorithm",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = DISABLED ALGORITHM AES);",
                "endpoint-algorithm",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = REQUIRED ALGORITHM AES AES);",
                "endpoint-algorithm",
            ),
            (
                "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ROLE = LEADER);",
                "endpoint-value",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (MESSAGE_FORWARDING = ON);",
                "endpoint-value",
            ),
            (
                "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, UNKNOWN_OPTION = 1) FOR TSQL ();",
                "endpoint-option",
            ),
            (
                "ALTER ENDPOINT ep FOR SERVICE_BROKER (ROLE = WITNESS);",
                "endpoint-option",
            ),
            (
                "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ROLE = WITNESS, MESSAGE_FORWARDING = ENABLED);",
                "endpoint-option",
            ),
            (
                "ALTER ENDPOINT ep FOR TSQL (AUTHENTICATION = WINDOWS);",
                "endpoint-option",
            ),
        ] {
            assert_eq!(rules(bad), [rule], "{bad}");
        }
    }
}
