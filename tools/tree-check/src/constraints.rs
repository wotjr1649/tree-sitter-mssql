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
        scalar_predicate(node, source, &mut out);
        surface_shapes(node, source, &mut out);
        match node.kind() {
            "invocation" => gap_invocation(node, source, &mut out),
            "binary_expression" | "term" | "exec_argument" => gap_scalar(node, source, &mut out),
            "identifier" | "object_reference" => gap_name(node, source, &mut out),
            "option" | "with_options" | "constraint" | "query_hint" | "table_hint" => {
                gap_options(node, source, &mut out)
            }
            "block"
            | "try_block"
            | "enable_trigger_statement"
            | "window_frame"
            | "create_event_session"
            | "alter_event_session"
            | "create_route"
            | "alter_route"
            | "alter_database"
            | "permission"
            | "trigger_event"
            | "create_index"
            | "create_server_audit"
            | "alter_server_audit" => gap_statement(node, source, &mut out),
            _ => {}
        }
        if matches!(
            node.kind(),
            "parenthesized_expression" | "drop_sequence" | "grant_statement"
        ) {
            ddl_rest_closing(node, source, &mut out);
        }
        if matches!(
            node.kind(),
            "create_credential"
                | "alter_credential"
                | "create_login"
                | "alter_login"
                | "create_user"
                | "alter_user"
                | "create_remote_service_binding"
                | "alter_remote_service_binding"
                | "create_resource_pool"
                | "alter_resource_pool"
                | "create_workload_group"
                | "alter_workload_group"
                | "alter_queue"
                | "alter_message_type"
                | "alter_authorization"
        ) {
            ddl_rest_alter(node, source, &mut out);
        }
        if matches!(
            node.kind(),
            "commit_statement"
                | "kill_statement"
                | "update_statistics_statement"
                | "sensitivity_classification_statement"
                | "signature_signer"
        ) {
            ddl_rest_misc(node, source, &mut out);
        }
        if matches!(node.kind(), "create_queue" | "alter_queue") {
            queue_activation(node, source, &mut out);
        }
        match node.kind() {
            "declare_cursor_statement" => query_cursor(node, source, &mut out),
            "set_statement" => {
                query_cursor(node, source, &mut out);
                query_offsets(node, source, &mut out);
            }
            "atomic_clause" => query_atomic(node, source, &mut out),
            "select" => {
                query_select(node, &mut out);
                ddl_rest_routes(node, source, &mut out);
            }
            "function_argument" => query_parameter(node, &mut out),
            "cast" => query_cast(node, &mut out),
            "openjson_schema" | "openxml_table_schema" => {
                query_rowset_schema(node, source, &mut out)
            }
            "delete" | "update" | "insert" => query_dml_source(node, &mut out),
            "assignment" => query_assignment(node, source, &mut out),
            _ => {}
        }
        if matches!(
            node.kind(),
            "create_procedure"
                | "alter_procedure"
                | "create_function"
                | "alter_function"
                | "create_trigger"
                | "alter_trigger"
        ) {
            query_module(node, &mut out);
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
            "grant_statement" => {
                column_permissions(node, source, &mut out);
                ddl_rest_permissions(node, &mut out);
            }
            "create_trigger" | "alter_trigger" => trigger_scope(node, source, &mut out),
            "invocation" => {
                builtin_arity(node, source, &mut out);
                expression_language(node, source, &mut out);
                expression_provider(node, source, &mut out);
                json_aggregate(node, source, &mut out);
            }
            "binary_expression" => like_escape(node, source, &mut out),
            "query_specification" => select_into_scope(node, &mut out),
            "set_operation" => nested_set_clauses(node, &mut out),
            "object_reference" | "field" => static_reference(node, source, &mut out),
            "create_function" | "alter_function" => scalar_function_return(node, &mut out),
            "window_specification" => window_frame_order(node, &mut out),
            "window_function" | "graph_match_expr" | "graph_shortest_path" | "graph_hop" => {
                ddl_rest_routes(node, source, &mut out);
            }
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
            "backup_statement" | "restore_statement" => backup_restore(node, source, &mut out),
            "create_index"
            | "create_aggregate"
            | "create_cryptographic_provider"
            | "create_event_session"
            | "create_asymmetric_key" => ddl_rest_create(node, source, &mut out),
            _ => {}
        }
        if node.kind() == "execute_statement" {
            ddl_rest_routes(node, source, &mut out);
            query_exec(node, &mut out);
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

/// Permission tails are verb-specific. SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170,
/// ddl-rest/permission-{grant-from,deny-from,grant-cascade,deny-with-grant,revoke-with-grant}
/// reject with Msg 102; permission-{grant,deny}-grant-option-for reject with Msg 156/102.
/// REVOKE accepts TO and FROM, with or without GRANT OPTION FOR, and CASCADE/AS.
fn ddl_rest_permissions(statement: Node, out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let parts: Vec<Node> = statement
        .named_children(&mut cursor)
        .filter(|node| !node.is_extra())
        .collect();
    let Some(verb) = parts.first().map(|node| node.kind()) else {
        return;
    };
    let has = |kind| parts.iter().any(|node| node.kind() == kind);
    let option_for = parts
        .get(1)
        .is_some_and(|node| node.kind() == "keyword_grant");
    if (verb != "keyword_revoke" && (has("keyword_from") || option_for))
        || (verb != "keyword_grant" && has("keyword_with"))
        || (verb == "keyword_grant" && has("keyword_cascade"))
    {
        out.push(violation(
            "ddl-rest-permission",
            "FROM and GRANT OPTION FOR require REVOKE; WITH GRANT OPTION requires GRANT; CASCADE requires DENY or REVOKE".into(),
            statement,
        ));
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

/// R18 last-forms/escape: ESCAPE follows an unparenthesized LIKE/NOT LIKE predicate (156).
/// R20 gap-forms/escape also rejects a search condition used as the escape character (102/156).
fn like_escape(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    if node
        .child_by_field_name("operator")
        .is_none_or(|n| n.kind() != "keyword_escape")
    {
        return;
    }
    let valid = node.child_by_field_name("left").is_some_and(|n| {
        n.kind() == "binary_expression"
            && n.child_by_field_name("operator")
                .is_some_and(|op| matches!(op.kind(), "keyword_like" | "not_like"))
    });
    let scalar = node
        .child_by_field_name("right")
        .is_none_or(|n| !is_condition(n, source));
    if !valid || !scalar {
        out.push(violation(
            "expression-like-escape",
            "ESCAPE must directly follow LIKE or NOT LIKE and take a scalar".into(),
            node,
        ));
    }
}

/// R18 last-forms/json*: argument count/shape, NULL/ORDER/RETURNING and window boundaries
/// measured on LocalDB 17.0.1000.7, levels 100-170 (102/156/174/10759).
fn json_aggregate(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(callee) = node.named_child(0) else {
        return;
    };
    let name = upper_text(callee, source);
    let array = name == "JSON_ARRAYAGG";
    let object = name == "JSON_OBJECTAGG";
    let mut c = node.walk();
    let args: Vec<_> = node.children_by_field_name("parameter", &mut c).collect();
    let pair = args.iter().any(|a| a.kind() == "json_key_value");
    let argument_end = node
        .children(&mut node.walk())
        .find(|n| n.kind() == ")")
        .map_or(node.end_byte(), |n| n.start_byte());
    let order = node
        .named_children(&mut node.walk())
        .any(|n| n.kind() == "order_by" && n.start_byte() < argument_end);
    let nulls = has_child(node, "json_null_clause");
    if !array && !object {
        if pair || order || nulls {
            out.push(violation(
                "expression-json-aggregate",
                "JSON pairs, NULL handling and argument ORDER BY require a JSON aggregate".into(),
                node,
            ));
        }
        return;
    }
    let within = has_child(node, "keyword_within");
    let distinct = has_child(node, "keyword_distinct");
    let window = node.parent().is_some_and(|p| p.kind() == "window_function");
    let returning = node
        .named_children(&mut node.walk())
        .find(|n| n.kind() == "returning_clause");
    let wrong_return = returning.is_some_and(|n| {
        n.named_children(&mut n.walk())
            .filter(|ty| !ty.is_extra())
            .nth(1)
            .is_none_or(|ty| upper_text(ty, source).trim_matches(['[', ']', '"']) != "JSON")
    });
    let wrong_arg = args.len() != 1
        || args.iter().any(|arg| {
            if object {
                arg.kind() != "json_key_value"
            } else {
                arg.kind() != "term"
                    || arg.child_by_field_name("alias").is_some()
                    || arg
                        .child_by_field_name("value")
                        .is_none_or(|v| v.kind() == "all_fields")
            }
        });
    if wrong_arg
        || wrong_return
        || has_child(node, "json_wrapper_clause")
        || (object && (order || distinct || within))
        || (array && distinct && (order || nulls || returning.is_some() || window))
        || (array && order && window)
        || (array && within && (order || nulls || returning.is_some() || distinct))
    {
        out.push(violation("expression-json-aggregate", "JSON aggregates require one value or key:value, measured NULL/ORDER/RETURNING clauses and window form".into(), node));
    }
}

/// R18 last-forms/into: any set branch may have INTO, even several branches; a nested
/// query, INSERT source or view body cannot (LocalDB 17.0.1000.7, levels 100-170, 156).
fn select_into_scope(node: Node, out: &mut Vec<Violation>) {
    if !has_child(node, "keyword_into") {
        return;
    }
    let mut parent = node.parent();
    while let Some(n) = parent {
        if matches!(
            n.kind(),
            "subquery" | "cte" | "insert" | "create_view" | "alter_view"
        ) {
            out.push(violation(
                "query-select-into-scope",
                "INTO is not allowed in a subquery, CTE, INSERT source or view".into(),
                node,
            ));
            return;
        }
        parent = n.parent();
    }
}

/// R15: LocalDB 17.0.1000.7, levels 100-170, expr-forms/language.
/// Keep argument positions and value restrictions here instead of multiplying grammar states.
fn expression_language(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(callee) = node.named_child(0) else {
        return;
    };
    let name = upper_text(callee, source);
    let mut cursor = node.walk();
    let args: Vec<_> = node
        .children_by_field_name("parameter", &mut cursor)
        .collect();
    let language: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| {
            arg.child_by_field_name("value")
                .is_some_and(|v| v.kind() == "field" && upper_text(v, source) == "LANGUAGE")
                && (arg.child_by_field_name("alias").is_some()
                    || matches!(
                        name.as_str(),
                        "CONTAINS" | "FREETEXT" | "CONTAINSTABLE" | "FREETEXTTABLE"
                    ))
        })
        .collect();
    if !language.is_empty() {
        let table = matches!(name.as_str(), "CONTAINSTABLE" | "FREETEXTTABLE");
        let expected = if table { 3 } else { 2 };
        let valid_name = table || matches!(name.as_str(), "CONTAINS" | "FREETEXT");
        let mut invalid = !valid_name
            || language.len() != 1
            || language[0].0 != expected
            || args.len() < expected + 1
            || args.len() > expected + 1 + usize::from(table);
        for (_, arg) in &language {
            invalid |= !arg.child_by_field_name("alias").is_some_and(|value| {
                let text = &source[value.byte_range()];
                string_literal(value, source)
                    || integer_literal(value, source)
                    || local_variable(value, source)
                    || (text.first() == Some(&b'"') && quoted_strings(node, source))
            });
        }
        if table && args.len() == expected + 2 {
            let top = args[expected + 1];
            invalid |= top.child_by_field_name("alias").is_some()
                || !top.child_by_field_name("value").is_some_and(|v| {
                    local_variable(v, source)
                        || (v.kind() == "literal"
                            && source[v.byte_range()].iter().all(u8::is_ascii_digit))
                });
        }
        if invalid {
            out.push(violation("expression-fulltext-language", "LANGUAGE needs a full-text function, its language argument position and a language name, LCID or local variable".into(), node));
        }
    }
}

/// R15 expr-forms/provider: credentials are confined to OPENROWSET's second argument.
fn expression_provider(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(callee) = node.named_child(0) else {
        return;
    };
    let name = upper_text(callee, source);
    let mut cursor = node.walk();
    let args: Vec<_> = node
        .children_by_field_name("parameter", &mut cursor)
        .collect();
    let connections: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| {
            let mut c = arg.walk();
            arg.children(&mut c).any(|n| n.kind() == ";")
        })
        .collect();
    if !connections.is_empty() {
        let mut invalid = name != "OPENROWSET"
            || connections.len() != 1
            || connections[0].0 != 1
            || args.len() != 3;
        for (_, connection) in &connections {
            let mut c = connection.walk();
            invalid |= connection
                .named_children(&mut c)
                .filter(|n| !n.is_extra())
                .any(|n| !string_literal(n, source));
        }
        invalid |= !args
            .first()
            .and_then(|n| n.child_by_field_name("value"))
            .is_some_and(|n| string_literal(n, source));
        if let Some(query) = args.get(2) {
            invalid |= query.child_by_field_name("alias").is_some()
                || !query.child_by_field_name("value").is_some_and(|v| {
                    string_literal(v, source)
                        || (v.kind() == "field" && !upper_text(v, source).starts_with('@'))
                });
        }
        if invalid {
            out.push(violation("expression-rowset-provider", "OPENROWSET takes provider, server;[user];[password], query or table; connection components are strings".into(), node));
        }
    }
}

fn string_literal(node: Node, source: &[u8]) -> bool {
    let text = &source[node.byte_range()];
    node.kind() == "literal"
        && (text.starts_with(b"'") || text.starts_with(b"N'") || text.starts_with(b"n'"))
}

fn integer_literal(node: Node, source: &[u8]) -> bool {
    let text = &source[node.byte_range()];
    node.kind() == "literal"
        && (!text.is_empty() && text.iter().all(u8::is_ascii_digit)
            || text
                .strip_prefix(b"0x")
                .or_else(|| text.strip_prefix(b"0X"))
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit)))
}

fn local_variable(node: Node, source: &[u8]) -> bool {
    let text = &source[node.byte_range()];
    matches!(node.kind(), "field" | "identifier")
        && text.starts_with(b"@")
        && !text.starts_with(b"@@")
}

// QUOTED_IDENTIFIER is explicit in the oracle's quoted-language neighbours.
fn quoted_strings(node: Node, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut strings = false;
    loop {
        let n = cursor.node();
        if n.start_byte() >= node.start_byte() {
            break;
        }
        if n.kind() == "set_statement" {
            let mut c = n.walk();
            let children: Vec<_> = n.named_children(&mut c).collect();
            if children.iter().any(|p| {
                p.kind() == "object_reference" && upper_text(*p, source) == "QUOTED_IDENTIFIER"
            }) {
                if children.iter().any(|p| p.kind() == "keyword_off") {
                    strings = true;
                }
                if children.iter().any(|p| p.kind() == "keyword_on") {
                    strings = false;
                }
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return strings;
            }
        }
    }
    strings
}

/// R15 expr-forms/set: an operand cannot carry its own trailing clauses, even with TOP.
fn nested_set_clauses(node: Node, out: &mut Vec<Violation>) {
    if node
        .parent()
        .is_some_and(|n| n.kind() == "query_specification")
    {
        let mut c = node.walk();
        if node.named_children(&mut c).any(|n| {
            matches!(
                n.kind(),
                "order_by" | "offset_fetch" | "option_clause" | "for_clause"
            )
        }) {
            out.push(violation(
                "expression-nested-set-clause",
                "a nested set operand has no trailing sort, hint or FOR clause".into(),
                node,
            ));
        }
    }
}

/// R15 expr-forms/static: type names have at most two parts; variables are not type/member names.
fn static_reference(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    if !node.children(&mut c).any(|n| n.kind() == "::") {
        return;
    }
    let Some(ty) = node.child_by_field_name("schema") else {
        return;
    };
    let Some(member) = node.child_by_field_name("name") else {
        return;
    };
    let mut c = ty.walk();
    let mut invalid = ty.children(&mut c).filter(|n| n.kind() == ".").count() > 1;
    for part in [ty, member] {
        let mut c = part.walk();
        invalid |= part
            .named_children(&mut c)
            .any(|n| source[n.byte_range()].starts_with(b"@") || upper_text(n, source) == "NULL");
    }
    if invalid {
        out.push(violation(
            "expression-static-reference",
            "a static type has at most schema.name and no variable type or member".into(),
            node,
        ));
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
/// INSERT and UPDATE. A WITH XMLNAMESPACES alone is not a CTE; FOR also permits a FROM-less
/// query (surface-fix-constraints, LocalDB 17.0.1000.7, levels 100-170).
fn cte_table_source(statement: Node, out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.named_children(&mut cursor).collect();
    if children.first().map(|n| n.kind()) != Some("keyword_with")
        || !children.iter().any(|n| n.kind() == "cte")
        || !children.iter().any(|n| n.kind() == "query_specification")
    {
        return;
    }
    let mut stack: Vec<Node> = children.into_iter().filter(|n| n.kind() != "cte").collect();
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "from" | "relation" | "for_clause") {
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

// Local statement constraints measured by query-forms on LocalDB 17.0.1000.7, levels 100-170.
// Keep the engine's surprising acceptances: repeated identical cursor flags, DATEFIRST=8 and
// DATEFORMAT=zzz pass PARSEONLY, so none is rejected here.
// offsets-bogus: Msg 195; offsets-valid/repeat: accepted (same engine and levels as above).
fn query_offsets(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let parts: Vec<Node> = node
        .named_children(&mut c)
        .filter(|n| !n.is_extra())
        .collect();
    if !parts
        .get(1)
        .is_some_and(|n| n.kind() == "object_reference" && upper_text(*n, source) == "OFFSETS")
    {
        return;
    }
    for part in parts.iter().skip(2) {
        if matches!(part.kind(), "keyword_on" | "keyword_off") {
            continue;
        }
        if ![
            "SELECT",
            "FROM",
            "ORDER",
            "COMPUTE",
            "TABLE",
            "PROCEDURE",
            "EXECUTE",
            "STATEMENT",
            "PARAM",
        ]
        .contains(&upper_text(*part, source).as_str())
        {
            out.push(violation(
                "query-offsets-options",
                "unknown OFFSETS statement type".into(),
                *part,
            ));
        }
    }
}

fn query_cursor(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let children: Vec<Node> = node
        .named_children(&mut c)
        .filter(|n| !n.is_extra())
        .collect();
    let Some(at) = children.iter().position(|n| n.kind() == "keyword_cursor") else {
        return;
    };
    let before: Vec<String> = children[..at]
        .iter()
        .map(|n| upper_text(*n, source))
        .collect();
    let mut flags = before
        .iter()
        .filter(|s| matches!(s.as_str(), "INSENSITIVE" | "SCROLL"))
        .cloned()
        .collect::<Vec<_>>();
    for n in &children[at + 1..] {
        if n.kind() == "keyword_for" {
            break;
        }
        flags.push(upper_text(*n, source));
    }
    let groups = [
        &["LOCAL", "GLOBAL"][..],
        &["FORWARD_ONLY", "SCROLL"][..],
        &["STATIC", "KEYSET", "DYNAMIC", "FAST_FORWARD"][..],
        &["READ_ONLY", "SCROLL_LOCKS", "OPTIMISTIC"][..],
    ];
    let known = [
        "LOCAL",
        "GLOBAL",
        "FORWARD_ONLY",
        "SCROLL",
        "STATIC",
        "KEYSET",
        "DYNAMIC",
        "FAST_FORWARD",
        "READ_ONLY",
        "SCROLL_LOCKS",
        "OPTIMISTIC",
        "TYPE_WARNING",
    ];
    let mut for_update = false;
    let mut scan = node.walk();
    loop {
        if scan.node().kind() == "for_clause" {
            let mut fc = scan.node().walk();
            for_update |= scan
                .node()
                .named_children(&mut fc)
                .any(|n| n.kind() == "keyword_update");
        }
        if scan.goto_first_child() {
            continue;
        }
        while !scan.goto_next_sibling() {
            if !scan.goto_parent() {
                break;
            }
        }
        if scan.node() == node {
            break;
        }
    }
    let invalid = flags
        .iter()
        .any(|f| !known.contains(&f.as_str()) && f != "INSENSITIVE")
        || children[at + 1..]
            .iter()
            .take_while(|n| n.kind() != "keyword_for")
            .any(|n| upper_text(*n, source) == "INSENSITIVE")
        || groups
            .iter()
            .any(|g| g.iter().filter(|v| flags.iter().any(|f| f == **v)).count() > 1)
        || flags.iter().any(|f| f == "FAST_FORWARD") && flags.iter().any(|f| f == "SCROLL")
        || flags.iter().any(|f| f == "INSENSITIVE") && for_update;
    // cursor-bogus/iso-after: 155/153; conflicting flags and ISO FOR UPDATE: 1048.
    if invalid {
        out.push(violation(
            "query-cursor-options",
            "unsupported or conflicting cursor options".into(),
            node,
        ));
    }
}

fn query_atomic(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let options: Vec<Node> = node
        .named_children(&mut c)
        .filter(|n| n.kind() == "atomic_option")
        .collect();
    let mut seen = Vec::new();
    let mut invalid = false;
    for option in options {
        let mut oc = option.walk();
        let parts: Vec<Node> = option.children(&mut oc).filter(|n| !n.is_extra()).collect();
        let isolation = parts
            .first()
            .is_some_and(|n| n.kind() == "keyword_transaction");
        let item = if isolation {
            option
        } else {
            option.named_child(0).unwrap_or(option)
        };
        let key = if isolation {
            "ISOLATION".into()
        } else {
            item.child_by_field_name("name")
                .map(|n| upper_text(n, source))
                .unwrap_or_default()
        };
        invalid |= seen.contains(&key);
        seen.push(key.clone());
        if isolation {
            continue;
        }
        let Some(value) = item.child_by_field_name("value") else {
            invalid = true;
            continue;
        };
        let mut ic = item.walk();
        invalid |= !item.children(&mut ic).any(|n| n.kind() == "=");
        invalid |= match key.as_str() {
            "LANGUAGE" => {
                value.kind() != "literal"
                    || !upper_text(value, source)
                        .trim_start_matches('N')
                        .starts_with('\'')
            }
            "DATEFIRST" => {
                value.kind() != "literal"
                    || !upper_text(value, source)
                        .bytes()
                        .all(|b| b.is_ascii_digit())
            }
            "DATEFORMAT" => !matches!(value.kind(), "identifier" | "literal"),
            "DELAYED_DURABILITY" => !matches!(value.kind(), "keyword_on" | "keyword_off"),
            _ => true,
        };
    }
    invalid |= !seen.iter().any(|k| k == "ISOLATION") || !seen.iter().any(|k| k == "LANGUAGE");
    let mut parent = node.parent();
    let mut module = false;
    while let Some(p) = parent {
        if matches!(
            p.kind(),
            "create_procedure"
                | "alter_procedure"
                | "create_function"
                | "alter_function"
                | "create_trigger"
                | "alter_trigger"
        ) {
            module = true;
            break;
        }
        parent = p.parent();
    }
    // atomic-no-*, repeat-*, unknown and malformed values: 10784/1039/195/102; plain-atomic: 102.
    if invalid || !module {
        out.push(violation(
            "query-atomic-options",
            "ATOMIC needs a module and unique, complete supported options".into(),
            node,
        ));
    }
}

fn query_module(node: Node, out: &mut Vec<Violation>) {
    let mut native = false;
    let mut atomic = false;
    let mut returns_null = false;
    let mut c = node.walk();
    loop {
        native |= c.node().kind() == "keyword_native_compilation";
        atomic |= c.node().kind() == "atomic_clause";
        returns_null |= c.node().kind() == "function_option"
            && c.node()
                .named_child(0)
                .is_some_and(|n| n.kind() == "keyword_returns");
        if c.goto_first_child() {
            continue;
        }
        while !c.goto_next_sibling() {
            if !c.goto_parent() {
                // function-native-returns-null and kit B04-r052/r089: 10794.
                if native && returns_null {
                    out.push(violation(
                        "query-native-null-input",
                        "native functions require CALLED ON NULL INPUT".into(),
                        node,
                    ));
                }
                // function-native-missing-atomic: 10783.
                if native && !atomic {
                    out.push(violation(
                        "query-native-atomic",
                        "NATIVE_COMPILATION needs BEGIN ATOMIC".into(),
                        node,
                    ));
                }
                return;
            }
        }
    }
}

// select-distinct-all: 156. Preserve quantified comparisons elsewhere in expressions.
fn query_select(node: Node, out: &mut Vec<Violation>) {
    let mut c = node.walk();
    if !node
        .named_children(&mut c)
        .any(|n| n.kind() == "keyword_distinct")
    {
        return;
    }
    let mut c = node.walk();
    let first = node
        .named_children(&mut c)
        .find(|n| n.kind() == "select_expression")
        .and_then(|n| n.named_child(0))
        .and_then(|n| n.child_by_field_name("value"));
    if first
        .and_then(|n| n.child_by_field_name("operator"))
        .is_some_and(|n| n.kind() == "keyword_all")
    {
        out.push(violation(
            "query-select-quantifier",
            "DISTINCT and ALL are mutually exclusive".into(),
            node,
        ));
    }
}

// function-cursor-param: 10725/178; procedure-cursor-param is accepted.
fn query_parameter(node: Node, out: &mut Vec<Violation>) {
    let mut c = node.walk();
    if !node
        .named_children(&mut c)
        .any(|n| n.kind() == "keyword_cursor")
    {
        return;
    }
    let mut parent = node.parent();
    while let Some(p) = parent {
        match p.kind() {
            "create_function" | "alter_function" => {
                out.push(violation(
                    "query-cursor-parameter",
                    "CURSOR parameters belong to procedures".into(),
                    node,
                ));
                return;
            }
            "create_procedure" | "alter_procedure" => return,
            _ => parent = p.parent(),
        }
    }
}

fn query_cast(node: Node, out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let parts: Vec<Node> = node.named_children(&mut c).collect();
    // cast-using/try-cast-using: 102. PARSE and TRY_PARSE accept USING.
    if parts.iter().any(|n| n.kind() == "keyword_using")
        && !parts
            .iter()
            .any(|n| matches!(n.kind(), "keyword_parse" | "keyword_try_parse"))
    {
        out.push(violation(
            "query-cast-using",
            "USING belongs to PARSE or TRY_PARSE".into(),
            node,
        ));
    }
}

fn query_exec(node: Node, out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let mut recompile = false;
    let mut results = false;
    for part in node.named_children(&mut c) {
        let repeated = match part.kind() {
            "keyword_recompile" => std::mem::replace(&mut recompile, true),
            "exec_result_sets" => std::mem::replace(&mut results, true),
            _ => false,
        };
        // exec-repeat-recompile/results: 102.
        if repeated {
            out.push(violation(
                "query-exec-options",
                "EXEC options occur at most once".into(),
                part,
            ));
        }
    }
}

fn query_rowset_schema(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let relation = node.prev_named_sibling();
    let call = relation.and_then(|r| {
        if r.kind() == "invocation" {
            Some(r)
        } else {
            r.named_child(0)
        }
    });
    let name = call
        .filter(|n| n.kind() == "invocation")
        .and_then(|n| n.named_child(0))
        .map(|n| upper_text(n, source))
        .unwrap_or_default();
    // rowset-56 and expr-forms/provider/schema: provider credentials take no WITH schema (102).
    if name == "OPENROWSET"
        && call.is_some_and(|call| {
            let mut c = call.walk();
            call.children_by_field_name("parameter", &mut c).any(|arg| {
                let mut c = arg.walk();
                arg.children(&mut c).any(|part| part.kind() == ";")
            })
        })
    {
        out.push(violation(
            "expression-rowset-provider",
            "OPENROWSET provider credentials take no WITH rowset schema".into(),
            node,
        ));
        return;
    }
    // R20 gap-forms/rowset: only these built-in rowsets take a WITH column schema.
    if !matches!(
        name.as_str(),
        "OPENJSON" | "OPENXML" | "OPENROWSET" | "PREDICT"
    ) {
        out.push(violation(
            "query-rowset-schema",
            "this source takes no WITH column schema".into(),
            node,
        ));
        return;
    }
    if node.kind() == "openxml_table_schema" {
        if name != "OPENXML" {
            out.push(violation(
                "query-rowset-schema",
                "a table schema reference belongs to OPENXML".into(),
                node,
            ));
        }
        return;
    }
    let mut c = node.walk();
    for column in node
        .named_children(&mut c)
        .filter(|n| n.kind() == "openjson_column")
    {
        let mut cc = column.walk();
        // predict-schema accepted; OPENJSON/OPENXML nullability: 156/319.
        if (name != "PREDICT"
            && column
                .named_children(&mut cc)
                .any(|n| matches!(n.kind(), "keyword_null" | "keyword_not" | "keyword_collate")))
            || (name != "OPENJSON" && has_child(column, "keyword_json"))
            || column.child_by_field_name("path").is_some_and(|p| {
                if name == "PREDICT" {
                    string_literal(p, source)
                } else if name == "OPENXML" || name == "OPENJSON" {
                    !string_literal(p, source)
                } else {
                    false
                }
            })
        {
            out.push(violation(
                "query-rowset-schema",
                "rowset column path, AS JSON and nullability must match the provider".into(),
                column,
            ));
            break;
        }
    }
}

fn query_dml_source(node: Node, out: &mut Vec<Violation>) {
    if node.parent().is_none_or(|n| n.kind() != "relation") {
        return;
    }
    let mut c = node.walk();
    let output = node
        .named_children(&mut c)
        .any(|n| n.kind() == "output_clause");
    let mut parent = node.parent();
    let mut insert = false;
    while let Some(p) = parent {
        if p.kind() == "insert" {
            insert = true;
            break;
        }
        parent = p.parent();
    }
    // insert-delete-no-output: 10716; select-delete-source: 156.
    if !output || !insert {
        out.push(violation(
            "query-dml-source",
            "a DML row source needs OUTPUT and an enclosing INSERT".into(),
            node,
        ));
    }
}

fn query_assignment(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    let comparison = right.kind() == "binary_expression"
        && right
            .child_by_field_name("operator")
            .is_some_and(|n| n.kind() == "=");
    // update-column-chain: 102; update-variable-chain/compound-chain accepted.
    if (right.kind() == "assignment" || comparison) && !upper_text(left, source).starts_with('@') {
        out.push(violation(
            "query-assignment-chain",
            "only a variable can capture a column assignment".into(),
            node,
        ));
    }
}

/// A position that needs a search condition holds a scalar expression (Msg 4145, "An expression of
/// non-boolean type specified in a context where a condition is expected"). The positions are every
/// `predicate` field (WHERE, JOIN ... ON, MERGE ... ON, WHEN [NOT] MATCHED ... AND), the condition of IF
/// and WHILE, the WHEN of a searched CASE (not a simple CASE, which has an `input`), the HAVING
/// expression and the first argument of IIF. Ported from grammar-kit's CheckTSQLContext (ADR-0004).
/// Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170: Msg 4145 for kit if-bit,
/// if-not-scalar, iif-scalar, merge-arith, merge-paren-scalar, merge-scalar and mut-0028x; every
/// engine-accepted oracle case passes.
fn scalar_predicate(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = node.walk();
    // An Extended Events WHERE shares the rule but takes event predicates, not search conditions.
    let in_event_session = || {
        let mut p = node.parent();
        while let Some(a) = p {
            if a.kind().ends_with("event_session") {
                return true;
            }
            p = a.parent();
        }
        false
    };
    let mut checked: Vec<Node> = if node.kind() == "where" && in_event_session() {
        Vec::new()
    } else {
        node.children_by_field_name("predicate", &mut cursor)
            .collect()
    };
    match node.kind() {
        "if_statement" | "while_statement" => checked.extend(node.child_by_field_name("condition")),
        "when_clause" => {
            let searched = node
                .parent()
                .is_some_and(|case| case.child_by_field_name("input").is_none());
            if searched {
                checked.extend(node.child_by_field_name("condition"));
            }
        }
        "having" => checked.extend(
            node.named_children(&mut cursor)
                .find(|n| n.kind() != "keyword_having"),
        ),
        "invocation" if invocation_name(node, source).as_deref() == Some("IIF") => {
            checked.extend(node.child_by_field_name("parameter"));
        }
        _ => {}
    }
    for n in checked {
        if !is_condition(n, source) {
            out.push(violation(
                "scalar-predicate",
                "a condition is expected here, not a scalar expression".into(),
                n,
            ));
        }
    }
}

/// The unqualified, upper-cased name of a function call, if it has one.
fn invocation_name(invocation: Node, source: &[u8]) -> Option<String> {
    invocation
        .named_child(0)
        .filter(|n| n.kind() == "object_reference" && n.named_child_count() == 1)
        .map(|n| upper_text(n, source))
}

/// Whether an expression is a search condition (true/false/unknown) rather than a scalar value.
fn is_condition(n: Node, source: &[u8]) -> bool {
    match n.kind() {
        "exists" | "graph_match_predicate" | "between_expression" => true,
        "invocation" => matches!(
            invocation_name(n, source).as_deref(),
            Some("UPDATE" | "CONTAINS" | "FREETEXT" | "REGEXP_LIKE")
        ),
        "term" | "parenthesized_expression" => n
            .child_by_field_name("value")
            .or_else(|| n.named_child(0))
            .is_some_and(|c| is_condition(c, source)),
        "binary_expression" => {
            let operator = n.child_by_field_name("operator").map(|o| o.kind());
            match operator {
                Some("keyword_and" | "keyword_or") => {
                    n.child_by_field_name("left")
                        .is_some_and(|c| is_condition(c, source))
                        && n.child_by_field_name("right")
                            .is_some_and(|c| is_condition(c, source))
                }
                Some(op) => matches!(
                    op,
                    "=" | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "<>"
                        | "!="
                        | "!<"
                        | "!>"
                        | "keyword_is"
                        | "is_not"
                        | "keyword_like"
                        | "not_like"
                        | "keyword_in"
                        | "not_in"
                        | "distinct_from"
                        | "not_distinct_from"
                        | "keyword_between"
                        | "keyword_escape"
                ),
                None => false,
            }
        }
        "unary_expression" => {
            n.child_by_field_name("operator").map(|o| o.kind()) == Some("keyword_not")
                && n.child_by_field_name("operand")
                    .is_some_and(|c| is_condition(c, source))
        }
        _ => false,
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

/// BACKUP and RESTORE option names, shapes and values that SQL Server checks while parsing; the
/// grammar reads every name as an identifier. Evidence: LocalDB 17.0.1000.7, levels 100-170,
/// oracle/probes/backup-restore.jsonl: `opt-*` (every option name bare, `= 1` and `= 'x'` in
/// BACKUP and RESTORE: unknown or other-statement names Msg 155, a flag given a value or a value
/// option without one Msg 153), `device-*` (DISK, TAPE, URL, DATABASE_SNAPSHOT, VIRTUAL_DEVICE
/// accepted; FILE 156, PIPE 155), `value-*` (a string, an integer or a variable; identifiers,
/// NULL, hex and decimals Msg 102/156/1080), `compression-*`, `encryption-*`, `filestream-*`,
/// `move-*`, `mark-*` and `cert-*` (Msg 102). Repeated and conflicting options are accepted
/// (`backup-duplicate-*`, `backup-init-noinit`, `restore-recovery-norecovery`), so no uniqueness.
fn backup_restore(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let restore = statement.kind() == "restore_statement";
    let Some(form) = statement.child(1) else {
        return;
    };
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.children(&mut cursor).collect();
    if form.kind() == "keyword_certificate" {
        backup_certificate(&children, source, out);
        return;
    }
    if has_child(statement, "keyword_key") {
        return; // MASTER, SERVICE MASTER and SYMMETRIC KEY: the grammar fixes their shape
    }
    // Options right after the name are files; after TO, MIRROR TO or FROM they are devices.
    let mut section = "";
    for node in children {
        match node.kind() {
            "identifier" if form.kind() == "keyword_group" => {
                if value_kind(node, source) == 'v' {
                    out.push(violation(
                        "backup-restore-device",
                        "BACKUP GROUP takes database names, not variables".into(),
                        node,
                    ));
                }
            }
            "identifier" if section.is_empty() => section = "file",
            "keyword_to" | "keyword_from" => section = "device",
            "option" if section == "file" => backup_file(node, form.kind(), source, out),
            "option" if section == "device" => backup_device(node, source, out),
            "with_clause" => {
                for option in direct_options(node) {
                    backup_option(option, restore, source, out);
                }
            }
            _ => {}
        }
    }
}

const BACKUP_FLAGS: &[&str] = &[
    "DIFFERENTIAL",
    "COPY_ONLY",
    "COMPRESSION",
    "NO_COMPRESSION",
    "FILE_SNAPSHOT",
    "NOINIT",
    "INIT",
    "NOSKIP",
    "SKIP",
    "NOFORMAT",
    "FORMAT",
    "NO_CHECKSUM",
    "CHECKSUM",
    "STOP_ON_ERROR",
    "CONTINUE_AFTER_ERROR",
    "RESTART",
    "REWIND",
    "NOREWIND",
    "UNLOAD",
    "NOUNLOAD",
    "NORECOVERY",
    "NO_TRUNCATE",
    "METADATA_ONLY",
    "SNAPSHOT",
    "NO_LOG",
    "STATS",
];
const BACKUP_VALUES: &[&str] = &[
    "STATS",
    "DESCRIPTION",
    "NAME",
    "CREDENTIAL",
    "EXPIREDATE",
    "RETAINDAYS",
    "MEDIADESCRIPTION",
    "MEDIANAME",
    "BLOCKSIZE",
    "BUFFERCOUNT",
    "MAXTRANSFERSIZE",
    "STANDBY",
    "PASSWORD",
    "MEDIAPASSWORD",
    "BACKUP_OPTIONS",
    "ENHANCEDINTEGRITY",
];
const RESTORE_FLAGS: &[&str] = &[
    "NO_CHECKSUM",
    "CHECKSUM",
    "STOP_ON_ERROR",
    "CONTINUE_AFTER_ERROR",
    "RESTART",
    "REWIND",
    "NOREWIND",
    "UNLOAD",
    "NOUNLOAD",
    "NORECOVERY",
    "RECOVERY",
    "METADATA_ONLY",
    "SNAPSHOT",
    "STATS",
    "REPLACE",
    "RESTRICTED_USER",
    "PARTIAL",
    "KEEP_REPLICATION",
    "KEEP_CDC",
    "ENABLE_BROKER",
    "ERROR_BROKER_CONVERSATIONS",
    "NEW_BROKER",
    "LOADHISTORY",
    "NO_LOG",
    "COMMIT_DIFFERENTIAL_BASE",
    "KEEP_TEMPORAL_RETENTION",
];
const RESTORE_VALUES: &[&str] = &[
    "STATS",
    "CREDENTIAL",
    "MEDIANAME",
    "BLOCKSIZE",
    "BUFFERCOUNT",
    "MAXTRANSFERSIZE",
    "STANDBY",
    "PASSWORD",
    "MEDIAPASSWORD",
    "FILE",
    "DBNAME",
    "STOPAT",
    "STOPATMARK",
    "STOPBEFOREMARK",
    "RESTORE_OPTIONS",
    "ENHANCEDINTEGRITY",
];

/// 's' string, 'i' integer (optionally signed), 'v' variable, '?' anything else.
fn value_kind(node: Node, source: &[u8]) -> char {
    let text = upper_text(node, source);
    let digits = text.trim_start_matches(['-', '+', ' ']);
    if node.kind() == "literal" && (text.starts_with('\'') || text.starts_with("N'")) {
        's'
    } else if matches!(node.kind(), "literal" | "unary_expression")
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        'i'
    } else if node.kind() == "identifier" && text.starts_with('@') {
        'v'
    } else {
        '?'
    }
}

/// `name = value` with nothing else, and the value's kind; None for any other shape.
fn assigned_value(option: Node, source: &[u8]) -> Option<char> {
    let value = option.child_by_field_name("value")?;
    (has_child(option, "=") && option.named_child_count() == 2).then(|| value_kind(value, source))
}

fn backup_file(option: Node, form: &str, source: &[u8], out: &mut Vec<Violation>) {
    let name = option_name(option, source);
    let valid = match name.as_str() {
        "READ_WRITE_FILEGROUPS" => option.named_child_count() == 1 && form != "keyword_log",
        "FILE" | "FILEGROUP" | "PAGE" => matches!(assigned_value(option, source), Some('s' | 'v')),
        _ => false,
    };
    if !valid {
        out.push(violation(
            "backup-restore-file",
            format!(
                "{name}: a file is FILE, FILEGROUP or PAGE = 'name' or @variable, or READ_WRITE_FILEGROUPS"
            ),
            option,
        ));
    }
}

fn backup_device(option: Node, source: &[u8], out: &mut Vec<Violation>) {
    let name = option_name(option, source);
    // A lone name is a logical backup device: a name or a variable.
    let valid = option.named_child_count() == 1
        || (matches!(
            name.as_str(),
            "DISK" | "TAPE" | "URL" | "DATABASE_SNAPSHOT" | "VIRTUAL_DEVICE"
        ) && matches!(assigned_value(option, source), Some('s' | 'v')));
    if !valid {
        out.push(violation(
            "backup-restore-device",
            format!(
                "{name}: a device is DISK, TAPE, URL, DATABASE_SNAPSHOT or VIRTUAL_DEVICE = 'path' or @variable"
            ),
            option,
        ));
    }
}

fn backup_option(option: Node, restore: bool, source: &[u8], out: &mut Vec<Violation>) {
    let name = option_name(option, source);
    let (flags, values) = if restore {
        (RESTORE_FLAGS, RESTORE_VALUES)
    } else {
        (BACKUP_FLAGS, BACKUP_VALUES)
    };
    let known = |list: &[&str]| list.contains(&name.as_str());
    let inner = direct_options(option);
    let value = option.child_by_field_name("value");
    let problem = if has_child(option, "(") {
        match (restore, name.as_str()) {
            (false, "COMPRESSION") => compression_list(&inner, source),
            (false, "ENCRYPTION") => encryption_list(&inner, source),
            (true, "FILESTREAM") => {
                let ok = inner.len() == 1
                    && option_name(inner[0], source) == "DIRECTORY_NAME"
                    && assigned_value(inner[0], source) == Some('s');
                (!ok).then_some("takes (DIRECTORY_NAME = 'name')")
            }
            _ => Some("takes no parenthesized list here"),
        }
    } else if has_child(option, "keyword_to") {
        let target = value.and_then(|v| v.next_named_sibling()?.next_named_sibling());
        if !restore {
            Some("belongs to RESTORE")
        } else if [value, target]
            .iter()
            .all(|t| t.is_some_and(|t| matches!(value_kind(t, source), 's' | 'v')))
        {
            None
        } else {
            Some("takes 'logical' TO 'path' (strings or variables)")
        }
    } else if !known(flags) && !known(values) {
        Some("is not an option of this statement")
    } else if value.is_none() {
        (!known(flags)).then_some("needs a value")
    } else if !known(values) {
        Some("takes no value")
    } else {
        let kind = value.map_or('?', |v| value_kind(v, source));
        let after = value
            .and_then(|v| v.next_named_sibling())
            .filter(|n| n.kind() == "keyword_after")
            .and_then(|n| n.next_named_sibling());
        if after.is_some() && !matches!(name.as_str(), "STOPATMARK" | "STOPBEFOREMARK") {
            Some("AFTER belongs to STOPATMARK and STOPBEFOREMARK")
        } else if after.is_some_and(|a| !matches!(value_kind(a, source), 's' | 'v')) {
            Some("AFTER takes a string or a variable")
        } else if restore && name == "FILE" && !matches!(kind, 'i' | 'v') {
            Some("takes a backup set number or a variable")
        } else if kind == '?' {
            Some("takes a string, an integer or a variable")
        } else {
            None
        }
    };
    if let Some(problem) = problem {
        out.push(violation(
            "backup-restore-option",
            format!("{name} {problem}"),
            option,
        ));
    }
}

/// COMPRESSION (ALGORITHM = MS_XPRESS | QAT_DEFLATE | ZSTD [, LEVEL = LOW | MEDIUM | HIGH]).
fn compression_list(inner: &[Node], source: &[u8]) -> Option<&'static str> {
    let ok = assigned_keyword(
        inner.first(),
        "ALGORITHM",
        &["MS_XPRESS", "QAT_DEFLATE", "ZSTD"],
        source,
    ) && (inner.len() == 1
        || (inner.len() == 2
            && assigned_keyword(inner.get(1), "LEVEL", &["LOW", "MEDIUM", "HIGH"], source)));
    (!ok).then_some(
        "takes (ALGORITHM = MS_XPRESS | QAT_DEFLATE | ZSTD [, LEVEL = LOW | MEDIUM | HIGH])",
    )
}

/// ENCRYPTION (ALGORITHM = AES_128 | AES_192 | AES_256 | TRIPLE_DES_3KEY, SERVER ... = name); the
/// grammar fixes the shape, and the server certificate or key is a name, not a variable.
fn encryption_list(inner: &[Node], source: &[u8]) -> Option<&'static str> {
    let algorithm = assigned_keyword(
        inner.first(),
        "ALGORITHM",
        &["AES_128", "AES_192", "AES_256", "TRIPLE_DES_3KEY"],
        source,
    );
    let server = inner
        .get(1)
        .and_then(|o| o.child_by_field_name("value"))
        .is_some_and(|v| value_kind(v, source) != 'v');
    (!(algorithm && server)).then_some(
        "takes (ALGORITHM = AES_128 | AES_192 | AES_256 | TRIPLE_DES_3KEY, SERVER CERTIFICATE | ASYMMETRIC KEY = name)",
    )
}

/// `key = WORD` with WORD one of `words`, written as a bare name.
fn assigned_keyword(option: Option<&Node>, key: &str, words: &[&str], source: &[u8]) -> bool {
    option.is_some_and(|o| {
        option_name(*o, source) == key
            && has_child(*o, "=")
            && o.child_by_field_name("value").is_some_and(|v| {
                v.kind() == "identifier" && words.contains(&upper_text(v, source).as_str())
            })
    })
}

/// BACKUP CERTIFICATE ... WITH: one FORMAT = 'string'; the PRIVATE KEY lists together need FILE =
/// 'string' with DER (also the default), and take FILE, ALGORITHM = 'string', ENCRYPTION BY and
/// DECRYPTION BY PASSWORD = 'string' once each. Any non-DER format permits an inline key at parse
/// time (surface-fix-constraints, LocalDB 17.0.1000.7, levels 100-170).
fn backup_certificate(children: &[Node], source: &[u8], out: &mut Vec<Violation>) {
    let Some(with) = children.iter().position(|n| n.kind() == "keyword_with") else {
        return;
    };
    let mut in_key = false;
    let mut any_key = false;
    let mut inline_key = false;
    let mut seen: Vec<String> = Vec::new();
    for node in &children[with..] {
        let (name, ok) = match node.kind() {
            "(" => {
                (in_key, any_key) = (true, true);
                continue;
            }
            ")" => {
                in_key = false;
                continue;
            }
            "option" => {
                let name = option_name(*node, source);
                if !in_key && name == "FORMAT" {
                    inline_key = node.child_by_field_name("value").is_some_and(|v| {
                        let text = upper_text(v, source);
                        let text = text.strip_prefix('N').unwrap_or(&text);
                        string_literal(v, source)
                            && text
                                .strip_prefix('\'')
                                .and_then(|s| s.strip_suffix('\''))
                                .is_some_and(|s| s.trim_end_matches(' ') != "DER")
                    });
                }
                let ok = if in_key {
                    matches!(name.as_str(), "FILE" | "ALGORITHM")
                } else {
                    name == "FORMAT"
                } && assigned_value(*node, source) == Some('s');
                (name, ok)
            }
            "encryption_mechanism" => {
                let name = node
                    .child(0)
                    .map_or(String::new(), |k| upper_text(k, source));
                let ok = node.named_child(2).is_some_and(|o| {
                    o.kind() == "option"
                        && option_name(o, source) == "PASSWORD"
                        && assigned_value(o, source) == Some('s')
                });
                (name, ok)
            }
            _ => continue,
        };
        let problem = if !ok {
            format!(
                "{name}: expected FORMAT = 'x' or PRIVATE KEY (FILE | ALGORITHM = 'x', {{ENCRYPTION | DECRYPTION}} BY PASSWORD = 'x')"
            )
        } else if seen.contains(&name) {
            format!("{name} appears more than once")
        } else {
            seen.push(name);
            continue;
        };
        out.push(violation("backup-certificate-options", problem, *node));
    }
    if any_key && !inline_key && !seen.iter().any(|n| n == "FILE") {
        out.push(violation(
            "backup-certificate-options",
            "DER PRIVATE KEY needs FILE = 'path'".into(),
            children[with],
        ));
    }
}

/// R17 ddl-rest/routes probes, LocalDB 17.0.1000.7, levels 100-170:
/// NULLS on SUM/ROW_NUMBER is Msg 16208; MATCH comma and ungrouped shortest path are Msg 102;
/// shortest-path lower bound != 1 is Msg 13942, upper bound 0 is Msg 13943; ALL DISTINCT is 156.
/// fixture-graph-before/shortest-fixed reject a pipe edge list (102); fixture-graph-after accepts OR of MATCH predicates.
/// Keep legacy raw trees and reject here.
fn ddl_rest_routes(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = node.walk();
    let children: Vec<_> = node
        .children(&mut cursor)
        .filter(|n| !n.is_extra())
        .collect();
    let problem = match node.kind() {
        "select" => {
            let all_distinct = children
                .iter()
                .find(|n| n.kind() == "select_expression")
                .and_then(|n| n.named_child(0))
                .and_then(|n| n.child_by_field_name("value"))
                .is_some_and(|n| {
                    n.child_by_field_name("operator")
                        .is_some_and(|n| n.kind() == "keyword_all")
                        && n.child_by_field_name("operand").is_some_and(|n| {
                            n.kind() == "field" && upper_text(n, source) == "DISTINCT"
                        })
                });
            all_distinct.then_some((
                "query-select-quantifier",
                "DISTINCT and ALL are mutually exclusive",
            ))
        }
        "window_function" => {
            let modifier = children
                .iter()
                .find(|n| matches!(n.kind(), "keyword_ignore" | "keyword_respect"));
            if modifier.is_none() {
                return;
            }
            let name = children
                .first()
                .and_then(|n| n.named_child(0))
                .map(|n| upper_text(n, source));
            if matches!(
                name.as_deref(),
                Some("FIRST_VALUE" | "LAST_VALUE" | "LAG" | "LEAD")
            ) {
                return;
            }
            Some((
                "ddl-rest-nulls-modifier",
                "IGNORE/RESPECT NULLS needs FIRST_VALUE, LAST_VALUE, LAG or LEAD",
            ))
        }
        "graph_hop" => children.iter().any(|n| n.kind() == "|").then_some((
            "ddl-rest-graph-edge",
            "a MATCH hop names one edge; combine complete MATCH predicates",
        )),
        "graph_match_expr" => children.iter().any(|n| n.kind() == ",").then_some((
            "ddl-rest-graph-match",
            "MATCH paths are joined with AND, not commas",
        )),
        "graph_shortest_path" => {
            if children.iter().any(|n| n.kind() == "graph_path") {
                Some((
                    "ddl-rest-graph-shortest",
                    "SHORTEST_PATH repeats a parenthesized hop group",
                ))
            } else if children
                .iter()
                .find(|n| n.kind() == "literal")
                .is_some_and(|n| upper_text(*n, source).parse::<u64>() != Ok(1))
            {
                Some((
                    "ddl-rest-graph-bound",
                    "the SHORTEST_PATH lower bound must be 1",
                ))
            } else if children
                .iter()
                .filter(|n| n.kind() == "literal")
                .nth(1)
                .is_some_and(|n| upper_text(*n, source).parse::<u64>() == Ok(0))
            {
                Some((
                    "ddl-rest-graph-bound",
                    "the SHORTEST_PATH upper bound must be positive",
                ))
            } else {
                None
            }
        }
        "execute_statement" => {
            if children
                .iter()
                .any(|n| matches!(n.kind(), "keyword_exec" | "keyword_execute"))
            {
                return;
            }
            let Some(batch) = node
                .parent()
                .and_then(|n| n.parent())
                .filter(|n| n.kind() == "batch")
            else {
                return;
            };
            let mut previous = batch.prev_named_sibling();
            while previous.is_some_and(|n| n.is_extra()) {
                previous = previous.and_then(|n| n.prev_named_sibling());
            }
            let follows_unterminated_batch = previous.is_some_and(|n| {
                let mut c = n.walk();
                n.named_children(&mut c)
                    .filter(|n| !n.is_extra())
                    .last()
                    .is_none_or(|n| n.kind() != "go_statement")
            });
            follows_unterminated_batch.then_some((
                "ddl-rest-implicit-exec",
                "omitting EXEC is allowed only first in a GO-delimited batch",
            ))
        }
        _ => None,
    };
    if let Some((rule, detail)) = problem {
        out.push(violation(rule, detail.into(), node));
    }
}

/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, ddl-rest/create-*:
/// aggregate-empty-args/default-arg: 102/10726; provider-binary and key-executable-no-file: 102;
/// event-session-empty: 102. The index neighbours below reject wrong storage/clause combinations
/// (102/156/1018), non-single JSON/SPATIAL columns (102), and unknown XML mode (487).
/// Spatial USING BOGUS and a three-value BOUNDING_BOX are PARSEONLY-accepted and stay unchecked.
fn ddl_rest_create(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    match node.kind() {
        "create_index" => rest_create_index(node, source, out),
        "create_aggregate" => {
            let mut c = node.walk();
            if let Some(args) = node
                .named_children(&mut c)
                .find(|n| n.kind() == "function_arguments")
            {
                let mut c = args.walk();
                let params: Vec<Node> = args
                    .named_children(&mut c)
                    .filter(|n| n.kind() == "function_argument")
                    .collect();
                if params.is_empty() || params.iter().any(|p| has_child(*p, "=")) {
                    out.push(violation(
                        "ddl-rest-aggregate",
                        "an aggregate needs at least one parameter and parameters have no defaults"
                            .into(),
                        args,
                    ));
                }
            }
        }
        "create_cryptographic_provider" => {
            let mut c = node.walk();
            if let Some(file) = node.named_children(&mut c).find(|n| n.kind() == "literal")
                && !(string_literal(file, source)
                    || (source.get(file.start_byte()) == Some(&b'"')
                        && quoted_strings(file, source)))
            {
                out.push(violation(
                    "ddl-rest-provider-file",
                    "a cryptographic provider's FILE is a string".into(),
                    file,
                ));
            }
        }
        "create_event_session" => {
            if !has_child(node, "event_session_event")
                && !has_child(node, "event_session_target")
                && !has_child(node, "with_options")
                && !has_child(node, "keyword_drop")
            {
                out.push(violation(
                    "ddl-rest-event-session",
                    "CREATE EVENT SESSION needs a body".into(),
                    node,
                ));
            }
        }
        "create_asymmetric_key" if has_child(node, "keyword_from") => {
            for option in direct_options(node) {
                if option_name(option, source) == "EXECUTABLE" {
                    out.push(violation(
                        "ddl-rest-key-source",
                        "FROM EXECUTABLE requires FILE = 'path'".into(),
                        option,
                    ));
                }
            }
        }
        _ => {}
    }
}

fn rest_create_index(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let children: Vec<Node> = node.children(&mut c).filter(|n| !n.is_extra()).collect();
    let storage = node
        .child_by_field_name("storage")
        .map(|n| upper_text(n, source))
        .unwrap_or_default();
    let xml = storage == "XML";
    let json = storage == "JSON";
    let spatial = storage == "SPATIAL";
    let primary = has_child(node, "keyword_primary") || storage == "PRIMARY";
    let selective = has_child(node, "keyword_selective") || storage == "SELECTIVE";
    let using = has_child(node, "keyword_using");
    let paths = has_child(node, "keyword_for");
    let columns = children.iter().find(|n| n.kind() == "ordered_columns");
    let wrong_columns = (json || spatial)
        && columns.is_some_and(|cols| {
            let mut c = cols.walk();
            cols.named_children(&mut c)
                .filter(|n| n.kind() == "column")
                .count()
                != 1
        });
    let wrong_mode = xml
        && using
        && children
            .iter()
            .position(|n| n.kind() == "keyword_for")
            .is_some_and(|i| {
                children.get(i + 1).is_some_and(|mode| {
                    mode.kind() != "("
                        && !matches!(
                            upper_text(*mode, source).as_str(),
                            "PATH" | "VALUE" | "PROPERTY"
                        )
                })
            });
    if ((primary || selective) && !xml)
        || (primary && using)
        || (selective && !paths)
        || (using && !xml && !spatial)
        || (paths && !xml && !json)
        || (json && has_child(node, "selective_xml_path"))
        || (json && has_child(node, "keyword_unique"))
        || wrong_columns
        || wrong_mode
        || columns.is_none() && !(storage == "COLUMNSTORE" && has_child(node, "keyword_clustered"))
    {
        out.push(violation(
            "ddl-rest-index-form",
            "index storage, columns and USING/FOR clauses must match".into(),
            node,
        ));
    }
}

/// Engine neighbours in ddl-rest, SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170:
/// missing CREDENTIAL IDENTITY/REMOTE BINDING USER, NUMANODE=AUTO, CREATE LOGIN OLD_PASSWORD,
/// bare QUEUE REORGANIZE WITH and incomplete ASYMMETRIC class are Msg 102; MAX_CPU_PERCENT=101
/// is Msg 1080; an unknown MESSAGE TYPE validation is Msg 195. PRIORITY_LEVEL=11 and an
/// application role without PASSWORD are accepted by PARSEONLY, so neither is restricted here.
/// extra-credential-alter-secret/scoped-alter-secret also reject missing IDENTITY (102), while
/// extra-remote-alter-anonymous accepts ALTER with ANONYMOUS alone, so only CREATE requires USER.
fn ddl_rest_alter(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let kind = statement.kind();
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement
        .children(&mut cursor)
        .filter(|n| !n.is_extra())
        .collect();
    let clauses: Vec<Node> = children
        .iter()
        .copied()
        .filter(|n| matches!(n.kind(), "with_clause" | "with_options"))
        .collect();
    for clause in clauses {
        if matches!(kind, "create_credential" | "alter_credential") {
            option_set(
                clause,
                source,
                "ddl-rest-credential",
                "ddl-rest-credential",
                &["IDENTITY"],
                &[],
                out,
            );
        }
        if kind == "create_remote_service_binding" {
            option_set(
                clause,
                source,
                "ddl-rest-remote-binding",
                "ddl-rest-remote-binding",
                &["USER"],
                &[],
                out,
            );
        }
        if kind == "alter_queue"
            && has_child(statement, "keyword_reorganize")
            && clause.kind() == "with_clause"
        {
            out.push(violation(
                "ddl-rest-queue",
                "REORGANIZE needs parentheses around its WITH options".into(),
                clause,
            ));
        }
        for option in direct_options(clause) {
            let name = option_name(option, source);
            let value = option.child_by_field_name("value");
            if kind == "create_login" {
                let mut c = option.walk();
                if option
                    .named_children(&mut c)
                    .any(|n| n.kind() == "identifier" && upper_text(n, source) == "OLD_PASSWORD")
                {
                    out.push(violation(
                        "ddl-rest-principal",
                        "OLD_PASSWORD belongs to ALTER, not CREATE LOGIN".into(),
                        option,
                    ));
                }
            }
            if matches!(
                kind,
                "create_remote_service_binding" | "alter_remote_service_binding"
            ) && name == "USER"
                && value.is_some_and(|n| n.kind() != "identifier")
            {
                out.push(violation(
                    "ddl-rest-remote-binding",
                    "USER is a principal name".into(),
                    option,
                ));
            }
            if matches!(kind, "create_resource_pool" | "alter_resource_pool") {
                if name == "AFFINITY"
                    && has_child(option, "keyword_numanode")
                    && !has_child(option, "(")
                {
                    out.push(violation(
                        "ddl-rest-resource-pool",
                        "NUMANODE needs a parenthesized range list".into(),
                        option,
                    ));
                }
                if name == "MAX_CPU_PERCENT"
                    && value.is_some_and(|n| {
                        upper_text(n, source).parse::<u32>().is_ok_and(|v| v > 100)
                    })
                {
                    out.push(violation(
                        "ddl-rest-resource-pool",
                        "MAX_CPU_PERCENT cannot exceed 100".into(),
                        option,
                    ));
                }
            }
            if matches!(kind, "create_workload_group" | "alter_workload_group")
                && name == "IMPORTANCE"
                && value.is_some_and(|n| {
                    !matches!(upper_text(n, source).as_str(), "LOW" | "MEDIUM" | "HIGH")
                })
            {
                out.push(violation(
                    "ddl-rest-workload-group",
                    "IMPORTANCE is LOW, MEDIUM or HIGH".into(),
                    option,
                ));
            }
        }
    }
    if kind == "alter_message_type"
        && let Some(eq) = children.iter().position(|n| n.kind() == "=")
    {
        let name = eq.checked_sub(1).and_then(|i| children.get(i));
        let value = children.get(eq + 1);
        let validation = value.map(|n| upper_text(*n, source)).unwrap_or_default();
        if name.is_some_and(|n| upper_text(*n, source) != "VALIDATION")
            || !matches!(
                validation.as_str(),
                "NONE" | "EMPTY" | "WELL_FORMED_XML" | "VALID_XML"
            )
            || (has_child(statement, "keyword_with") && validation != "VALID_XML")
        {
            out.push(violation("ddl-rest-message-type", "VALIDATION is NONE, EMPTY, WELL_FORMED_XML or VALID_XML; only VALID_XML takes a schema collection".into(), statement));
        }
    }
    if kind == "alter_authorization"
        && let Some(separator) = children.iter().position(|n| n.kind() == "::")
        && separator > 0
        && upper_text(children[separator - 1], source) == "ASYMMETRIC"
    {
        out.push(violation(
            "ddl-rest-authorization",
            "the ASYMMETRIC class needs KEY".into(),
            children[separator - 1],
        ));
    }
}

/// LocalDB 17.0.1000.7, levels 100-170, ddl-rest probes:
/// commit-bad/duplicate/invalid/multi/missing (102); kill-queryString (102);
/// stats-wrong/unknown (155), stats-both (1052); sensitivity-badRank (16114),
/// badLabel (102), duplicate (16104), unknown (16101), name (16103);
/// signature-unknown/both (102). Names and catalog/variable resolution remain contextual.
fn ddl_rest_misc(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let container = node.child_by_field_name("options").or_else(|| {
        let mut c = node.walk();
        node.named_children(&mut c)
            .find(|n| matches!(n.kind(), "with_options" | "with_clause"))
    });
    let options = container.map(direct_options).unwrap_or_default();
    let reject = |out: &mut Vec<Violation>, at: Node, detail: &str| {
        out.push(violation("ddl-rest-misc", detail.into(), at));
    };
    match node.kind() {
        "commit_statement" => {
            if let Some(clause) = container
                && (options.len() != 1
                    || option_name(options[0], source) != "DELAYED_DURABILITY"
                    || !options[0]
                        .child_by_field_name("value")
                        .is_some_and(|n| matches!(upper_text(n, source).as_str(), "ON" | "OFF"))
                    || !has_child(options[0], "="))
            {
                reject(
                    out,
                    clause,
                    "COMMIT WITH takes DELAYED_DURABILITY = ON or OFF",
                );
            }
        }
        "kill_statement" => {
            if has_child(node, "keyword_query") {
                let mut c = node.walk();
                if node
                    .named_children(&mut c)
                    .filter(|n| n.kind() == "literal")
                    .any(|n| value_kind(n, source) != 'i')
                {
                    reject(
                        out,
                        node,
                        "a query notification subscription needs an integer or ALL",
                    );
                }
            }
        }
        "update_statistics_statement" => {
            let allowed = [
                "FULLSCAN",
                "SAMPLE",
                "RESAMPLE",
                "STATS_STREAM",
                "ROWCOUNT",
                "PAGECOUNT",
                "ALL",
                "COLUMNS",
                "INDEX",
                "NORECOMPUTE",
                "INCREMENTAL",
                "PERSIST_SAMPLE_PERCENT",
                "MAXDOP",
                "AUTO_DROP",
            ];
            let names: Vec<_> = options.iter().map(|n| option_name(*n, source)).collect();
            if names.iter().any(|n| !allowed.contains(&n.as_str()))
                || names.iter().any(|n| n == "COLUMNS") && names.iter().any(|n| n == "INDEX")
                || options.iter().any(|n| {
                    has_child(*n, "keyword_partitions") && option_name(*n, source) != "RESAMPLE"
                })
            {
                reject(
                    out,
                    node,
                    "invalid UPDATE STATISTICS option or partition suffix",
                );
            }
        }
        "sensitivity_classification_statement" => {
            let mut c = node.walk();
            for name in node
                .named_children(&mut c)
                .filter(|n| n.kind() == "object_reference")
            {
                let mut nc = name.walk();
                if !name.children(&mut nc).any(|n| n.kind() == ".") {
                    reject(
                        out,
                        name,
                        "a sensitivity classification target needs table.column",
                    );
                }
            }
            let mut seen = Vec::new();
            for option in options {
                let name = option_name(option, source);
                let value = option.child_by_field_name("value");
                let valid = has_child(option, "=")
                    && match name.as_str() {
                        "LABEL" | "LABEL_ID" | "INFORMATION_TYPE" | "INFORMATION_TYPE_ID" => {
                            value.is_some_and(|n| value_kind(n, source) == 's')
                        }
                        "RANK" => value.is_some_and(|n| {
                            matches!(
                                upper_text(n, source).as_str(),
                                "NONE" | "LOW" | "MEDIUM" | "HIGH" | "CRITICAL"
                            )
                        }),
                        _ => false,
                    };
                if !valid || seen.contains(&name) {
                    reject(
                        out,
                        option,
                        "invalid or duplicate sensitivity classification option",
                    );
                }
                seen.push(name);
            }
        }
        "signature_signer" => {
            if let Some(clause) = container {
                let valid = options.len() == 1
                    && !(has_child(node, "keyword_symmetric")
                        && option_name(options[0], source) == "PASSWORD")
                    && match option_name(options[0], source).as_str() {
                        "PASSWORD" => assigned_value(options[0], source) == Some('s'),
                        "SIGNATURE" => {
                            options[0].child_by_field_name("value").is_some_and(|n| {
                                n.kind() == "literal" && upper_text(n, source).starts_with("0X")
                            }) && has_child(options[0], "=")
                        }
                        _ => false,
                    };
                if !valid {
                    reject(
                        out,
                        clause,
                        "a signature signer takes one PASSWORD string or SIGNATURE binary value",
                    );
                }
            }
        }
        _ => {}
    }
}

/// Closing neighbours, SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170:
/// ddl-rest/closing-xe-* rejects a parenthesized scalar predicate source (102), but accepts
/// parentheses around the whole comparison. closing-drop-sequence-* rejects database/server
/// prefixes (166/117). closing-ag-* rejects CONNECT on AG (102), accepting ALTER and CONTROL.
/// The measured AG CONNECT result differs from the current documentation; do not infer a
/// complete permission whitelist. The quoted-class probes reject joined multiword classes (102).
fn ddl_rest_closing(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    match node.kind() {
        "parenthesized_expression" => {
            let mut cursor = node.walk();
            let mut parts = node.named_children(&mut cursor).filter(|n| !n.is_extra());
            if !parts.next().is_some_and(|n| n.kind() == "field") || parts.next().is_some() {
                return;
            }
            let mut ancestor = node.parent();
            let mut predicate = false;
            while let Some(parent) = ancestor {
                match parent.kind() {
                    "where" => predicate = true,
                    "event_session_event" => {
                        if predicate {
                            out.push(violation(
                                "ddl-rest-xe-source",
                                "an XE scalar predicate source is not parenthesized; the whole comparison may be".into(),
                                node,
                            ));
                        }
                        return;
                    }
                    "statement" | "batch" => return,
                    _ => {}
                }
                ancestor = parent.parent();
            }
        }
        "drop_sequence" => {
            let mut cursor = node.walk();
            for name in node
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "object_reference")
            {
                let mut cursor = name.walk();
                if name
                    .children(&mut cursor)
                    .filter(|n| n.kind() == ".")
                    .count()
                    > 1
                {
                    out.push(violation(
                        "ddl-rest-drop-sequence",
                        "DROP SEQUENCE takes name or schema.name, without database or server prefixes".into(),
                        name,
                    ));
                }
            }
        }
        "grant_statement" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node
                .children(&mut cursor)
                .filter(|n| !n.is_extra())
                .collect();
            let Some(on) = children.iter().position(|n| n.kind() == "keyword_on") else {
                return;
            };
            let Some(separator) = children[on + 1..].iter().position(|n| n.kind() == "::") else {
                return;
            };
            let class = &children[on + 1..on + 1 + separator];
            if let [word] = class {
                let text = word.utf8_text(source).unwrap_or_default();
                if (text.starts_with('[') || text.starts_with('"'))
                    && text.split_whitespace().count() > 1
                {
                    out.push(violation(
                        "ddl-rest-securable-class",
                        "a multiword securable class is separate unquoted words, not one quoted identifier".into(),
                        *word,
                    ));
                }
                return;
            }
            let [availability, group] = class else {
                return;
            };
            if upper_text(*availability, source) != "AVAILABILITY"
                || upper_text(*group, source) != "GROUP"
            {
                return;
            }
            for permission in children.iter().filter(|n| n.kind() == "permission") {
                let mut cursor = permission.walk();
                let mut words = permission
                    .named_children(&mut cursor)
                    .filter(|n| !n.is_extra());
                if words
                    .next()
                    .is_some_and(|n| upper_text(n, source) == "CONNECT")
                    && words.next().is_none()
                {
                    out.push(violation(
                        "ddl-rest-ag-connect",
                        "CONNECT ON AVAILABILITY GROUP is rejected by the measured SQL Server parsers".into(),
                        *permission,
                    ));
                }
            }
        }
        _ => {}
    }
}

/// Surface gaps, LocalDB 17.0.1000.7, levels 100-170 (surface-fix-gaps): a BULK rowset
/// needs an alias (491); without an explicit schema, CSV needs FORMATFILE and the legacy
/// form needs FORMATFILE or SINGLE_* (472/15808). SET assignments cannot feed UNION,
/// EXCEPT or INTERSECT (10734). CREATE SCHEMA owns its batch (111/156).
/// surface-fix-ddl-classification measures crypto WITH (156/319), reserved USER principals (156),
/// scalar CTAS variables (112) and CETAS options (102); platform CTAS/CETAS clauses remain outside.
fn surface_shapes(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "invocation" if invocation_name(node, source).as_deref() == Some("OPENROWSET") => {
            let mut c = node.walk();
            let args: Vec<_> = node.children_by_field_name("parameter", &mut c).collect();
            let bulk = args
                .first()
                .and_then(|a| a.child_by_field_name("value"))
                .is_some_and(|v| upper_text(v, source) == "BULK");
            if !bulk || node.parent().is_none_or(|p| p.kind() != "relation") {
                return;
            }
            let relation = node.parent().unwrap();
            let schema = relation
                .next_named_sibling()
                .filter(|n| n.kind() == "openjson_schema");
            let alias = relation.child_by_field_name("alias").is_some()
                || schema
                    .and_then(|s| s.next_named_sibling())
                    .is_some_and(|n| {
                        n.kind() == "identifier"
                            || n.kind() == "keyword_as"
                                && n.next_named_sibling()
                                    .is_some_and(|n| n.kind() == "identifier")
                    });
            let mut format = None;
            let mut formatfile = false;
            let mut single = false;
            for arg in args {
                let v = arg.child_by_field_name("value").unwrap_or(arg);
                if v.kind() == "binary_expression" {
                    if let Some(left) = v.child_by_field_name("left") {
                        match upper_text(left, source).as_str() {
                            "FORMATFILE" => formatfile = true,
                            "FORMAT" => {
                                format = v
                                    .child_by_field_name("right")
                                    .map(|r| upper_text(r, source))
                            }
                            _ => {}
                        }
                    }
                } else {
                    single |= matches!(
                        upper_text(v, source).as_str(),
                        "SINGLE_BLOB" | "SINGLE_CLOB" | "SINGLE_NCLOB"
                    );
                }
            }
            !alias
                || schema.is_none()
                    && !formatfile
                    && match format.as_deref() {
                        None => !single,
                        Some("'CSV'" | "N'CSV'") => true,
                        _ => false,
                    }
        }
        "term"
            if node
                .parent()
                .is_some_and(|p| p.kind() == "select_expression") =>
        {
            node.child_by_field_name("value").is_some_and(|v| {
                v.kind() == "binary_expression"
                    && v.child_by_field_name("operator")
                        .is_some_and(|n| n.kind() == "=")
                    && v.child_by_field_name("left")
                        .is_some_and(|n| local_variable(n, source))
                    && ancestor(node, "query_specification")
                        .and_then(|q| q.parent())
                        .is_some_and(|p| p.kind() == "set_operation")
            })
        }
        "alter_cryptographic_provider" => has_child(node, "with_clause"),
        "field" => {
            node.named_child_count() == 1
                && local_variable(node, source)
                && ancestor(node, "query_specification").is_some()
                && ancestor(node, "create_table").is_some_and(|t| {
                    has_child(t, "keyword_as")
                        && !has_child(t, "keyword_external")
                        && !has_child(t, "column_definitions")
                })
        }
        "create_table" if has_child(node, "keyword_external") && has_child(node, "keyword_as") => {
            node.named_children(&mut node.walk())
                .find(|n| n.kind() == "with_options")
                .map(direct_options)
                .unwrap_or_default()
                .iter()
                .any(|o| {
                    !matches!(
                        option_name(*o, source).as_str(),
                        // Preserve documented SQL Server/Synapse and MI options, even when this engine lacks the platform feature.
                        "LOCATION"
                            | "DATA_SOURCE"
                            | "FILE_FORMAT"
                            | "PARTITION"
                            | "REJECT_TYPE"
                            | "REJECT_VALUE"
                            | "REJECT_SAMPLE_VALUE"
                    )
                })
        }
        "identifier"
            if upper_text(node, source) == "USER"
                && node.parent().is_some_and(|p| p.kind() == "grant_statement") =>
        {
            let mut previous = node.prev_named_sibling();
            let mut principal = false;
            while let Some(n) = previous {
                if matches!(n.kind(), "keyword_to" | "keyword_from") {
                    principal = true;
                    break;
                }
                if matches!(n.kind(), "keyword_on" | "keyword_as") {
                    break;
                }
                previous = n.prev_named_sibling();
            }
            principal
        }
        "create_schema" => {
            let Some(statement) = node.parent().filter(|p| p.kind() == "statement") else {
                return;
            };
            if statement.parent().is_none_or(|p| p.kind() != "batch") {
                return;
            }
            let previous = statement.prev_named_sibling().filter(|n| !n.is_extra());
            let next = statement
                .next_named_sibling()
                .filter(|n| !n.is_extra() && n.kind() != "go_statement");
            previous.is_some()
                || next.is_some_and(|next| {
                    let terminated = statement.next_sibling().is_some_and(|n| n.kind() == ";");
                    let element = next.named_child(0);
                    terminated
                        || element.is_none_or(|n| match n.kind() {
                            "create_table" => has_child(n, "keyword_external"),
                            "create_view" | "grant_statement" => false,
                            _ => true,
                        })
                })
        }
        "alter_search_property_list" if has_child(node, "keyword_add") => {
            let options = node
                .named_children(&mut node.walk())
                .find(|n| n.kind() == "with_options")
                .map(direct_options)
                .unwrap_or_default();
            let names: Vec<_> = options.iter().map(|n| option_name(*n, source)).collect();
            let ordered = names == ["PROPERTY_SET_GUID", "PROPERTY_INT_ID"]
                || names
                    == [
                        "PROPERTY_SET_GUID",
                        "PROPERTY_INT_ID",
                        "PROPERTY_DESCRIPTION",
                    ];
            !ordered
                || options.iter().any(|o| {
                    if option_name(*o, source) == "PROPERTY_INT_ID" {
                        o.child_by_field_name("value").is_none_or(|v| {
                            v.kind() != "literal"
                                || !upper_text(v, source).bytes().all(|b| b.is_ascii_digit())
                                || !upper_text(v, source).parse::<i32>().is_ok_and(|n| n >= 0)
                        }) || !has_child(*o, "=")
                    } else {
                        assigned_value(*o, source) != Some('s')
                    }
                })
        }
        _ => false,
    };
    if invalid {
        out.push(violation("surface-shape", "rowset, set assignment, schema batch or search-property shape is rejected by the engine".into(), node));
    }
}

/// R20 gap-forms, LocalDB 17.0.1000.7, levels 100-170: tree-local call boundaries
/// (102/156/174). Qualified user functions are not built-ins; write targets take constants
/// or local variables, not general expressions. Keep the tree for queries and consumers.
fn gap_invocation(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let args: Vec<_> = node.children_by_field_name("parameter", &mut c).collect();
    let name = invocation_name(node, source).unwrap_or_default();

    let target = node
        .parent()
        .is_some_and(|p| matches!(p.kind(), "delete" | "update"));
    let invalid = match name.as_str() {
        "[COUNT]" | "\"COUNT\"" | "[COALESCE]" | "\"COALESCE\"" => true,
        "COALESCE" => args.len() < 2,
        "DATENAME" => args.first().is_some_and(|a| {
            let v = a.child_by_field_name("value").unwrap_or(*a);
            let text = upper_text(v, source);
            let text = text
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .or_else(|| text.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                .unwrap_or(&text);
            v.kind() != "field"
                || !matches!(
                    text,
                    "YEAR"
                        | "YY"
                        | "YYYY"
                        | "QUARTER"
                        | "QQ"
                        | "Q"
                        | "MONTH"
                        | "MM"
                        | "M"
                        | "DAYOFYEAR"
                        | "DY"
                        | "Y"
                        | "DAY"
                        | "DD"
                        | "D"
                        | "WEEK"
                        | "WK"
                        | "WW"
                        | "WEEKDAY"
                        | "DW"
                        | "W"
                        | "HOUR"
                        | "HH"
                        | "MINUTE"
                        | "MI"
                        | "N"
                        | "SECOND"
                        | "SS"
                        | "S"
                        | "MILLISECOND"
                        | "MS"
                        | "MICROSECOND"
                        | "MCS"
                        | "NANOSECOND"
                        | "NS"
                        | "TZOFFSET"
                        | "TZ"
                        | "ISO_WEEK"
                        | "ISOWK"
                        | "ISOWW"
                )
        }),
        "OPENQUERY" => {
            args.len() != 2
                || args.first().is_some_and(|a| {
                    let v = a.child_by_field_name("value").unwrap_or(*a);
                    v.kind() != "field"
                        || v.named_child_count() != 1
                        || v.named_child(0).is_none_or(|n| {
                            n.kind() != "identifier" || source[n.byte_range()].starts_with(b"@")
                        })
                })
        }
        "OPENROWSET" if target => args
            .first()
            .and_then(|a| a.child_by_field_name("value"))
            .is_some_and(|v| upper_text(v, source) == "BULK"),
        "PREDICT" => {
            node.parent().is_some_and(|p| p.kind() == "relation")
                && node
                    .parent()
                    .and_then(|p| p.next_named_sibling())
                    .is_none_or(|n| n.kind() != "openjson_schema")
        }
        _ if target => args
            .iter()
            .any(|a| !gap_constant(a.child_by_field_name("value").unwrap_or(*a), source)),
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-call-shape",
            "call arguments or rowset schema do not match this built-in or write target".into(),
            node,
        ));
    }
    if matches!(
        name.as_str(),
        "CONTAINS" | "FREETEXT" | "CONTAINSTABLE" | "FREETEXTTABLE"
    ) || name == "PROPERTY"
        && ancestor(node, "invocation").is_some_and(|p| {
            matches!(
                invocation_name(p, source).as_deref(),
                Some("CONTAINS" | "CONTAINSTABLE")
            )
        })
    {
        let pos = if name.ends_with("TABLE") { 2 } else { 1 };
        if let Some(arg) = args.get(pos) {
            let v = arg.child_by_field_name("value").unwrap_or(*arg);
            if v.utf8_text(source).unwrap_or_default().starts_with('"')
                && !quoted_strings(v, source)
            {
                out.push(violation(
                    "gap-fulltext-string",
                    "a double-quoted fulltext string requires QUOTED_IDENTIFIER OFF".into(),
                    v,
                ));
            }
        }
    }
}

fn gap_constant(node: Node, source: &[u8]) -> bool {
    node.kind() == "literal"
        || local_variable(node, source)
        || node.kind() == "keyword_default"
        || node.kind() == "unary_expression"
            && node
                .child_by_field_name("operator")
                .is_some_and(|n| n.kind() == "-")
            && node
                .child_by_field_name("operand")
                .is_some_and(|n| n.kind() == "literal")
}

/// R20 comparison/* and static/*: a comparison's operands and a select-list value
/// are scalars, not search conditions (102); EXEC accepts -1 but not +1.
fn gap_scalar(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "exec_argument" => node
            .child_by_field_name("value")
            .is_some_and(|n| n.kind() == "unary_expression" && has_child(n, "+")),
        "term" => {
            node.parent()
                .is_some_and(|p| p.kind() == "select_expression")
                && node.child_by_field_name("value").is_some_and(|n| {
                    is_condition(n, source)
                        && !(n.kind() == "binary_expression"
                            && n.child_by_field_name("operator")
                                .is_some_and(|o| o.kind() == "=")
                            && n.child_by_field_name("left").is_some_and(|l| {
                                string_literal(l, source)
                                    || l.kind() == "field"
                                        && l.named_child_count() == 1
                                        && l.named_child(0)
                                            .is_some_and(|id| id.kind() == "identifier")
                            }))
                })
        }
        "binary_expression" => {
            node.child_by_field_name("operator").is_some_and(|n| {
                matches!(
                    n.kind(),
                    "=" | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "<>"
                        | "!="
                        | "!<"
                        | "!>"
                        | "keyword_is"
                        | "is_not"
                        | "keyword_in"
                        | "not_in"
                )
            }) && ["left", "right"].iter().any(|f| {
                node.child_by_field_name(f)
                    .is_some_and(|n| is_condition(n, source))
            })
        }
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-scalar-shape",
            "this position needs a scalar expression or an EXEC constant without unary plus".into(),
            node,
        ));
    }
}

fn ancestor<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut n = node.parent();
    while let Some(p) = n {
        if p.kind() == kind {
            return Some(p);
        }
        n = p.parent();
    }
    None
}

/// R20 name/qualification, engine 117/156: XML collections have at most schema.name,
/// HISTORY_TABLE has exactly schema.name; XE qualified names have no empty components.
fn gap_name(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(parent) = node.parent() else {
        return;
    };
    if node.kind() == "identifier" {
        let text = upper_text(node, source);
        let in_name = matches!(
            parent.kind(),
            "field" | "object_reference" | "column_definition" | "term"
        );
        // TABLE on VECTOR_SEARCH's left-hand assignment is syntax, not a column name.
        let vector_key = parent.parent().is_some_and(|n| {
            n.kind() == "binary_expression"
                && n.child_by_field_name("left") == Some(parent)
                && ancestor(n, "invocation")
                    .is_some_and(|a| invocation_name(a, source).as_deref() == Some("VECTOR_SEARCH"))
        });
        if (text == "COALESCE"
            && parent.kind() == "object_reference"
            && parent.child_by_field_name("schema").is_some())
            || in_name
                && matches!(
                    text.as_str(),
                    "TABLE" | "ESCAPE" | "FROM" | "FILLFACTOR" | "EXTERNAL"
                )
                && !vector_key
                && ancestor(node, "set_statement").is_none()
            || text == "EXTERNAL" && parent.kind() == "relation"
            || text == "USER"
                && (parent.kind() == "object_reference"
                    && parent.parent().is_none_or(|p| p.kind() != "field")
                    || parent.kind() == "relation"
                    || parent.kind() == "field"
                        && parent.named_child_count() > 1
                        && parent.named_child(0) != Some(node))
        {
            out.push(violation(
                "gap-reserved-name",
                "this reserved name must be delimited".into(),
                node,
            ));
        }
        return;
    }
    let mut c = node.walk();
    let parts = node
        .named_children(&mut c)
        .filter(|n| !n.is_extra())
        .count();
    let mut c = node.walk();
    let dots = node.children(&mut c).filter(|n| n.kind() == ".").count();
    let collection = matches!(
        parent.kind(),
        "create_xml_schema_collection"
            | "alter_xml_schema_collection"
            | "drop_xml_schema_collection"
    );
    let history = parent.kind() == "option" && option_name(parent, source) == "HISTORY_TABLE";
    let xe = matches!(
        parent.kind(),
        "event_session_event" | "event_session_target"
    );
    let xml_null = parent.kind() == "alter_table"
        && node
            .prev_named_sibling()
            .is_some_and(|n| n.kind() == "keyword_xml")
        && upper_text(node, source) == "NULL";
    if (collection && (dots > 1 || parts != dots + 1))
        || (history && (dots != 1 || parts != 2))
        || (xe && parts != dots + 1)
        || xml_null
    {
        out.push(violation(
            "gap-name-parts",
            "name qualification or XML schema name is invalid in this position".into(),
            node,
        ));
    }
    // XE ACTION names are direct object_reference children too, unlike nested type names.
    if xe && dots > 2 {
        out.push(violation(
            "gap-name-parts",
            "XE event/action names have at most three parts".into(),
            node,
        ));
    }
    if parent.kind() == "drop_xml_schema_collection" && has_child(parent, "keyword_if") {
        out.push(violation(
            "gap-xml-drop",
            "DROP XML SCHEMA COLLECTION has no IF EXISTS clause".into(),
            parent,
        ));
    }
}

/// R20 gap-forms/hints, maxdop, table and index: Msg 1018/304/1080/10737/13539/
/// 35310/10798/122/153/155. These limits depend only on the option's owner and value.
fn gap_options(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "table_hint" => {
            let target = dml_target(node);
            let merge_source =
                node.parent().is_some_and(|n| n.kind() == "merge") && target.is_none();
            merge_source
                || !has_child(node, "keyword_with")
                    && (target.is_some() || {
                        let mut c = node.walk();
                        node.named_children(&mut c)
                            .any(|n| n.kind() == "table_hint_item" && has_child(n, "keyword_index"))
                    })
        }
        "query_hint" => {
            let mut c = node.walk();
            let parts: Vec<_> = node
                .named_children(&mut c)
                .filter(|n| !n.is_extra())
                .collect();
            parts
                .first()
                .is_some_and(|n| upper_text(*n, source) == "DISABLE_OPTIMIZED_PLAN_FORCING")
                || parts
                    .first()
                    .is_some_and(|n| upper_text(*n, source) == "MAXDOP")
                    && (parts.len() != 2
                        || !upper_text(parts[1], source)
                            .bytes()
                            .all(|b| b.is_ascii_digit())
                        || !upper_text(parts[1], source)
                            .parse::<u32>()
                            .is_ok_and(|n| n <= 32767))
        }
        "constraint" => {
            has_child(node, "keyword_columnstore")
                && ancestor(node, "table_var_declaration").is_some()
        }
        "with_options" => {
            let mut seen = false;
            node.parent().is_some_and(|p| p.kind() == "create_table")
                && direct_options(node).iter().any(|o| {
                    option_name(*o, source) == "SYSTEM_VERSIONING"
                        && std::mem::replace(&mut seen, true)
                })
        }
        "option" => {
            let name = option_name(node, source);
            let value = node.child_by_field_name("value");
            let table = ancestor(node, "alter_table");
            let index = ancestor(node, "alter_index");
            let column_key = node
                .parent()
                .and_then(|p| p.parent())
                .is_some_and(|p| matches!(p.kind(), "column_definition" | "constraint"));
            let partitions = has_child(node, "keyword_partitions");
            // The engine checks ON PARTITIONS only on the first WITH option (gap-forms/partition).
            let compression_partition = partitions
                && node
                    .prev_named_sibling()
                    .is_some_and(|n| n.kind() == "keyword_with")
                && index.is_some()
                && (!matches!(name.as_str(), "DATA_COMPRESSION" | "XML_COMPRESSION")
                    || index.is_some_and(|i| {
                        let mut c = i.walk();
                        let parts: Vec<_> = i.named_children(&mut c).collect();
                        !parts.windows(2).any(|w| {
                            w[0].kind() == "keyword_partition" && upper_text(w[1], source) == "ALL"
                        })
                    }));
            let row_key = node.parent().and_then(|p| p.parent()).is_some_and(|p| {
                !has_child(p, "keyword_columnstore")
                    && (has_child(p, "keyword_primary") || has_child(p, "keyword_unique"))
            });
            let bad_key_option = column_key
                && row_key
                && (name == "COMPRESSION_DELAY"
                    || name == "DATA_COMPRESSION"
                        && value.is_some_and(|v| upper_text(v, source).starts_with("COLUMNSTORE")));
            let table_rebuild = table.is_some_and(|t| has_child(t, "keyword_rebuild"))
                && matches!(name.as_str(), "RESUMABLE" | "OPTIMIZE_FOR_SEQUENTIAL_KEY");
            let lock_escalation = table.is_some()
                && value.is_some_and(|n| upper_text(n, source) == "TABLE")
                && name != "LOCK_ESCALATION";
            let policy =
                ancestor(node, "alter_security_policy").is_some() && name == "SCHEMABINDING";
            let external = ancestor(node, "create_external_file_format").is_some()
                && (name == "FORMAT_TYPE"
                    && value.is_some_and(|v| {
                        matches!(upper_text(v, source).as_str(), "JSON" | "RCFILE")
                    })
                    || name == "PARSER_VERSION");
            let source_option = ancestor(node, "alter_external_data_source").is_some()
                && name == "RESOURCE_MANAGER_LOCATION";
            let history = name == "HISTORY_TABLE"
                && value.is_some_and(|v| v.kind() != "object_reference" || !has_child(v, "."));
            compression_partition
                || history
                || (ancestor(node, "create_table")
                    .is_some_and(|t| has_child(t, "keyword_external"))
                    && matches!(name.as_str(), "DATA_SOURCE" | "FILE_FORMAT")
                    && value.is_none_or(|v| {
                        v.kind() != "identifier" || upper_text(v, source).starts_with('@')
                    }))
                || bad_key_option
                || table_rebuild
                || lock_escalation
                || policy
                || external
                || source_option
        }
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-option-shape",
            "option value, placement, range or repetition is not accepted here".into(),
            node,
        ));
    }
    if node.kind() == "with_options"
        && direct_options(node)
            .iter()
            .any(|n| option_name(*n, source) == "ONLINE")
        && ancestor(node, "create_index").is_some_and(|i| {
            i.child_by_field_name("storage")
                .is_some_and(|s| upper_text(s, source) == "JSON")
        })
    {
        out.push(violation(
            "gap-json-index",
            "JSON indexes do not support ONLINE".into(),
            node,
        ));
    }
}

/// R20 gap-forms/blocks, xe, route, cache, window, permission and trigger: LocalDB
/// 17.0.1000.7, levels 100-170. Empty CATCH and final unterminated ENABLE are accepted.
fn gap_statement(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "block" | "try_block" => {
            let mut c = node.walk();
            !node
                .named_children(&mut c)
                .any(|n| matches!(n.kind(), "statement" | "block" | "label_statement"))
        }
        "enable_trigger_statement" => node.parent().is_some_and(|s| {
            let mut next = s.next_sibling();
            while next.is_some_and(|n| n.is_extra()) {
                next = next.and_then(|n| n.next_sibling());
            }
            next.is_some_and(|n| n.kind() != ";" && n.kind() != "go_statement")
        }),
        "window_frame" => {
            !has_child(node, "keyword_between")
                && node.named_child(1).is_some_and(|f| {
                    has_child(f, "keyword_unbounded") && has_child(f, "keyword_following")
                })
        }
        "create_event_session" => !has_child(node, "event_session_event"),
        "alter_event_session" => {
            let mut c = node.walk();
            node.named_children(&mut c)
                .filter(|n| !n.is_extra())
                .count()
                == 6
        }
        "create_route" | "alter_route" => {
            let options = node
                .named_children(&mut node.walk())
                .find(|n| n.kind() == "with_clause")
                .map(direct_options)
                .unwrap_or_default();
            let mut seen = Vec::new();
            let mut duplicate = false;
            for opt in &options {
                let name = option_name(*opt, source);
                duplicate |= seen.contains(&name);
                seen.push(name);
            }
            duplicate || node.kind() == "create_route" && !seen.iter().any(|s| s == "ADDRESS")
        }
        "alter_database" => {
            let handle = node
                .named_children(&mut node.walk())
                .find(|n| n.kind() == "keyword_procedure_cache")
                .and_then(next_option_unit);
            handle
                .is_some_and(|n| n.kind() != "literal" || !upper_text(n, source).starts_with("0X"))
                || has_child(node, "keyword_scoped")
                    && direct_options(node).iter().any(|n| {
                        matches!(
                            option_name(*n, source).as_str(),
                            "ALLOW_BUILTIN_TVF_IN_ALL_COMPAT_LEVELS" | "ALLOW_STALE_VECTOR_INDEX"
                        )
                    })
        }
        "create_index" => {
            node.child_by_field_name("storage")
                .is_some_and(|s| upper_text(s, source) == "JSON")
                && has_child(node, "on_filegroup")
        }
        "create_server_audit" | "alter_server_audit" => {
            let mut c = node.walk();
            let parts: Vec<_> = node
                .named_children(&mut c)
                .filter(|n| !n.is_extra())
                .collect();
            parts.windows(2).any(|w| {
                w[0].kind() == "keyword_to"
                    && matches!(
                        upper_text(w[1], source).as_str(),
                        "URL" | "EXTERNAL_MONITOR"
                    )
            })
        }
        "permission" => {
            if node.parent().is_none_or(|p| p.kind() != "grant_statement") {
                return;
            }
            let mut c = node.walk();
            let words: Vec<_> = node
                .named_children(&mut c)
                .filter(|n| !n.is_extra() && n.kind() != "list")
                .map(|n| upper_text(n, source))
                .collect();
            let head = words.first().map(String::as_str).unwrap_or_default();
            !matches!(
                head,
                "ALL"
                    | "ALTER"
                    | "AUTHENTICATE"
                    | "BACKUP"
                    | "CHECKPOINT"
                    | "CONNECT"
                    | "CONTROL"
                    | "CREATE"
                    | "DELETE"
                    | "EXECUTE"
                    | "EXEC"
                    | "IMPERSONATE"
                    | "INSERT"
                    | "RECEIVE"
                    | "REFERENCES"
                    | "SELECT"
                    | "SEND"
                    | "SHOWPLAN"
                    | "SHUTDOWN"
                    | "SUBSCRIBE"
                    | "TAKE"
                    | "UNMASK"
                    | "UPDATE"
                    | "VIEW"
                    | "ADMINISTER"
                    | "EXTERNAL"
                    | "ENABLE"
                    | "KILL"
            ) || matches!(
                words.join(" ").as_str(),
                "CREATE APPLICATION ROLE" | "ALTER ANY DATABASE SCOPED CREDENTIAL"
            )
        }
        "trigger_event" => {
            let event = upper_text(node, source);
            !matches!(
                event.as_str(),
                "INSERT" | "UPDATE" | "DELETE" | "ALTER_SERVER_CONFIGURATION"
            ) && !SERVER_ONLY_EVENTS.contains(&event.as_str())
                && !DATABASE_TRIGGER_EVENTS.contains(&event.as_str())
        }
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-statement-shape",
            "statement body, event, option or permission shape is rejected by the engine".into(),
            node,
        ));
    }
}

// Fixed event/group names measured by the context/trig-* oracle enumeration.
const DATABASE_TRIGGER_EVENTS: &[&str] = &[
    "ADD_INFORMATION_PROTECTION",
    "ADD_ROLE_MEMBER",
    "ADD_SENSITIVITY_CLASSIFICATION",
    "ADD_SIGNATURE",
    "ADD_SIGNATURE_SCHEMA_OBJECT",
    "ALTER_APPLICATION_ROLE",
    "ALTER_ASSEMBLY",
    "ALTER_ASYMMETRIC_KEY",
    "ALTER_AUDIT",
    "ALTER_AUTHORIZATION_DATABASE",
    "ALTER_BROKER_PRIORITY",
    "ALTER_CERTIFICATE",
    "ALTER_COLUMN_ENCRYPTION_KEY",
    "ALTER_DATABASE_AUDIT_SPECIFICATION",
    "ALTER_DATABASE_ENCRYPTION_KEY",
    "ALTER_DATABASE_SCOPED_CONFIGURATION",
    "ALTER_EXTENDED_PROPERTY",
    "ALTER_EXTERNAL_LANGUAGE",
    "ALTER_EXTERNAL_LIBRARY",
    "ALTER_FULLTEXT_CATALOG",
    "ALTER_FULLTEXT_INDEX",
    "ALTER_FULLTEXT_STOPLIST",
    "ALTER_FUNCTION",
    "ALTER_INDEX",
    "ALTER_MASTER_KEY",
    "ALTER_MESSAGE_TYPE",
    "ALTER_PARTITION_FUNCTION",
    "ALTER_PARTITION_SCHEME",
    "ALTER_PLAN_GUIDE",
    "ALTER_PROCEDURE",
    "ALTER_QUEUE",
    "ALTER_REMOTE_SERVICE_BINDING",
    "ALTER_ROLE",
    "ALTER_ROUTE",
    "ALTER_SCHEMA",
    "ALTER_SEARCH_PROPERTY_LIST",
    "ALTER_SECURITY_POLICY",
    "ALTER_SEQUENCE",
    "ALTER_SERVICE",
    "ALTER_SYMMETRIC_KEY",
    "ALTER_TABLE",
    "ALTER_TRIGGER",
    "ALTER_USER",
    "ALTER_VIEW",
    "ALTER_XML_SCHEMA_COLLECTION",
    "BIND_DEFAULT",
    "BIND_RULE",
    "CREATE_APPLICATION_ROLE",
    "CREATE_ASSEMBLY",
    "CREATE_ASYMMETRIC_KEY",
    "CREATE_AUDIT",
    "CREATE_BROKER_PRIORITY",
    "CREATE_CERTIFICATE",
    "CREATE_COLUMN_ENCRYPTION_KEY",
    "CREATE_COLUMN_MASTER_KEY",
    "CREATE_CONTRACT",
    "CREATE_DATABASE_AUDIT_SPECIFICATION",
    "CREATE_DATABASE_ENCRYPTION_KEY",
    "CREATE_DEFAULT",
    "CREATE_EVENT_NOTIFICATION",
    "CREATE_EXTENDED_PROPERTY",
    "CREATE_EXTERNAL_LANGUAGE",
    "CREATE_EXTERNAL_LIBRARY",
    "CREATE_EXTERNAL_STREAM",
    "CREATE_FULLTEXT_CATALOG",
    "CREATE_FULLTEXT_INDEX",
    "CREATE_FULLTEXT_STOPLIST",
    "CREATE_FUNCTION",
    "CREATE_INDEX",
    "CREATE_JSON_INDEX",
    "CREATE_MASTER_KEY",
    "CREATE_MESSAGE_TYPE",
    "CREATE_PARTITION_FUNCTION",
    "CREATE_PARTITION_SCHEME",
    "CREATE_PLAN_GUIDE",
    "CREATE_PROCEDURE",
    "CREATE_QUEUE",
    "CREATE_REMOTE_SERVICE_BINDING",
    "CREATE_ROLE",
    "CREATE_ROUTE",
    "CREATE_RULE",
    "CREATE_SCHEMA",
    "CREATE_SEARCH_PROPERTY_LIST",
    "CREATE_SECURITY_POLICY",
    "CREATE_SEQUENCE",
    "CREATE_SERVICE",
    "CREATE_SPATIAL_INDEX",
    "CREATE_STATISTICS",
    "CREATE_SYMMETRIC_KEY",
    "CREATE_SYNONYM",
    "CREATE_TABLE",
    "CREATE_TRIGGER",
    "CREATE_TYPE",
    "CREATE_USER",
    "CREATE_VECTOR_INDEX",
    "CREATE_VIEW",
    "CREATE_XML_INDEX",
    "CREATE_XML_SCHEMA_COLLECTION",
    "DDL_APPLICATION_ROLE_EVENTS",
    "DDL_ASSEMBLY_EVENTS",
    "DDL_ASYMMETRIC_KEY_EVENTS",
    "DDL_AUTHORIZATION_DATABASE_EVENTS",
    "DDL_BROKER_PRIORITY_EVENTS",
    "DDL_CERTIFICATE_EVENTS",
    "DDL_COLUMN_ENCRYPTION_KEY_EVENTS",
    "DDL_COLUMN_MASTER_KEY_EVENTS",
    "DDL_CONTRACT_EVENTS",
    "DDL_CRYPTO_SIGNATURE_EVENTS",
    "DDL_DATABASE_AUDIT_EVENTS",
    "DDL_DATABASE_AUDIT_SPECIFICATION_EVENTS",
    "DDL_DATABASE_ENCRYPTION_KEY_EVENTS",
    "DDL_DATABASE_LEVEL_EVENTS",
    "DDL_DATABASE_SECURITY_EVENTS",
    "DDL_DEFAULT_EVENTS",
    "DDL_EVENT_NOTIFICATION_EVENTS",
    "DDL_EXTENDED_PROPERTY_EVENTS",
    "DDL_EXTERNAL_LANGUAGE_EVENTS",
    "DDL_EXTERNAL_STREAM_EVENTS",
    "DDL_FIDO_EVENTS",
    "DDL_FULLTEXT_CATALOG_EVENTS",
    "DDL_FULLTEXT_STOPLIST_EVENTS",
    "DDL_FUNCTION_EVENTS",
    "DDL_GDR_DATABASE_EVENTS",
    "DDL_INDEX_EVENTS",
    "DDL_LIBRARY_EVENTS",
    "DDL_MASTER_KEY_EVENTS",
    "DDL_MESSAGE_TYPE_EVENTS",
    "DDL_PARTITION_EVENTS",
    "DDL_PARTITION_FUNCTION_EVENTS",
    "DDL_PARTITION_SCHEME_EVENTS",
    "DDL_PLAN_GUIDE_EVENTS",
    "DDL_PROCEDURE_EVENTS",
    "DDL_QUEUE_EVENTS",
    "DDL_REMOTE_SERVICE_BINDING_EVENTS",
    "DDL_ROLE_EVENTS",
    "DDL_ROUTE_EVENTS",
    "DDL_RULE_EVENTS",
    "DDL_SCHEMA_EVENTS",
    "DDL_SEARCH_PROPERTY_LIST_EVENTS",
    "DDL_SECURITY_POLICY_EVENTS",
    "DDL_SENSITIVITY_EVENTS",
    "DDL_SEQUENCE_EVENTS",
    "DDL_SERVICE_EVENTS",
    "DDL_SSB_EVENTS",
    "DDL_STATISTICS_EVENTS",
    "DDL_SYMMETRIC_KEY_EVENTS",
    "DDL_SYNONYM_EVENTS",
    "DDL_TABLE_EVENTS",
    "DDL_TABLE_VIEW_EVENTS",
    "DDL_TRIGGER_EVENTS",
    "DDL_TYPE_EVENTS",
    "DDL_USER_EVENTS",
    "DDL_VIEW_EVENTS",
    "DDL_XML_SCHEMA_COLLECTION_EVENTS",
    "DENY_DATABASE",
    "DROP_APPLICATION_ROLE",
    "DROP_ASSEMBLY",
    "DROP_ASYMMETRIC_KEY",
    "DROP_AUDIT",
    "DROP_BROKER_PRIORITY",
    "DROP_CERTIFICATE",
    "DROP_COLUMN_ENCRYPTION_KEY",
    "DROP_COLUMN_MASTER_KEY",
    "DROP_CONTRACT",
    "DROP_DATABASE_AUDIT_SPECIFICATION",
    "DROP_DATABASE_ENCRYPTION_KEY",
    "DROP_DEFAULT",
    "DROP_EVENT_NOTIFICATION",
    "DROP_EXTENDED_PROPERTY",
    "DROP_EXTERNAL_LANGUAGE",
    "DROP_EXTERNAL_LIBRARY",
    "DROP_EXTERNAL_STREAM",
    "DROP_FULLTEXT_CATALOG",
    "DROP_FULLTEXT_INDEX",
    "DROP_FULLTEXT_STOPLIST",
    "DROP_FUNCTION",
    "DROP_INDEX",
    "DROP_INFORMATION_PROTECTION",
    "DROP_MASTER_KEY",
    "DROP_MESSAGE_TYPE",
    "DROP_PARTITION_FUNCTION",
    "DROP_PARTITION_SCHEME",
    "DROP_PLAN_GUIDE",
    "DROP_PROCEDURE",
    "DROP_QUEUE",
    "DROP_REMOTE_SERVICE_BINDING",
    "DROP_ROLE",
    "DROP_ROLE_MEMBER",
    "DROP_ROUTE",
    "DROP_RULE",
    "DROP_SCHEMA",
    "DROP_SEARCH_PROPERTY_LIST",
    "DROP_SECURITY_POLICY",
    "DROP_SENSITIVITY_CLASSIFICATION",
    "DROP_SEQUENCE",
    "DROP_SERVICE",
    "DROP_SIGNATURE",
    "DROP_SIGNATURE_SCHEMA_OBJECT",
    "DROP_STATISTICS",
    "DROP_SYMMETRIC_KEY",
    "DROP_SYNONYM",
    "DROP_TABLE",
    "DROP_TRIGGER",
    "DROP_TYPE",
    "DROP_USER",
    "DROP_VIEW",
    "DROP_XML_SCHEMA_COLLECTION",
    "GRANT_DATABASE",
    "RENAME",
    "REVOKE_DATABASE",
    "UNBIND_DEFAULT",
    "UNBIND_RULE",
    "UNDO_DROP",
    "UPDATE_STATISTICS",
];

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(sql: &str) -> Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
        p.parse(sql, None).unwrap()
    }

    fn rules(sql: &str) -> Vec<&'static str> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
        let tree = p.parse(sql, None).unwrap();
        check(&tree, sql.as_bytes())
            .into_iter()
            .map(|v| v.rule)
            .collect()
    }

    #[test]
    fn surface_constraint_neighbours_match_the_engine() {
        let verdict: serde_json::Value = serde_json::from_str(include_str!(
            "../../../oracle/verdicts/localdb-17.0.1000.7.surface-fix-constraints.json"
        ))
        .unwrap();
        for line in include_str!("../../../oracle/probes/surface-fix-constraints.jsonl").lines() {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            let id = case["id"].as_str().unwrap();
            let sql = case["source"].as_str().unwrap();
            let accepted = verdict["verdicts"][id]
                .as_object()
                .unwrap()
                .values()
                .any(|v| v.as_array().is_some_and(|a| a.is_empty()));
            assert_eq!(rules(sql).is_empty(), accepted, "{id}: {:?}", rules(sql));
        }
    }

    #[test]
    fn surface_gap_neighbours_match_the_engine() {
        let verdict: serde_json::Value = serde_json::from_str(include_str!(
            "../../../oracle/verdicts/localdb-17.0.1000.7.surface-fix-gaps.json"
        ))
        .unwrap();
        for line in include_str!("../../../oracle/probes/surface-fix-gaps.jsonl").lines() {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            let id = case["id"].as_str().unwrap();
            let sql = case["source"].as_str().unwrap();
            let accepted = verdict["verdicts"][id]
                .as_object()
                .unwrap()
                .values()
                .any(|v| v.as_array().is_some_and(|a| a.is_empty()));
            let tree = parse(sql);
            assert_eq!(
                !tree.root_node().has_error() && rules(sql).is_empty(),
                accepted,
                "{id}: {:?}",
                rules(sql)
            );
        }
    }

    #[test]
    fn surface_ddl_classification_neighbours_match_the_engine() {
        let verdict: serde_json::Value = serde_json::from_str(include_str!(
            "../../../oracle/verdicts/localdb-17.0.1000.7.surface-fix-ddl-classification.json"
        ))
        .unwrap();
        for line in
            include_str!("../../../oracle/probes/surface-fix-ddl-classification.jsonl").lines()
        {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            let id = case["id"].as_str().unwrap();
            let sql = case["source"].as_str().unwrap();
            let accepted = verdict["verdicts"][id]
                .as_object()
                .unwrap()
                .values()
                .any(|v| v.as_array().is_some_and(|a| a.is_empty()));
            let tree = parse(sql);
            if accepted {
                assert!(
                    !tree.root_node().has_error() && rules(sql).is_empty(),
                    "{id}: {:?}",
                    rules(sql)
                );
            } else if id.ends_with("crypto_with_enable")
                || id.ends_with("crypto_with_disable")
                || id.ends_with("crypto_with_file")
                || id.ends_with("grant_language_user")
                || id.ends_with("grant_library_user")
                || id.ends_with("cetas_format_options")
                || id.ends_with("cetas_format_options_file")
                || id.ends_with("cetas_bad_option")
                || id.ends_with("ctas_declared_variable")
                || id.ends_with("ctas_undeclared_variable")
                || id.ends_with("ctas_variable_where")
                || id.ends_with("ctas_variable_function")
                || id.ends_with("ctas_subquery_variable")
                || id.ends_with("ctas_cte_variable")
            {
                assert!(!rules(sql).is_empty(), "{id}");
            } else {
                assert!(
                    rules(sql).is_empty(),
                    "platform/context rejection {id}: {:?}",
                    rules(sql)
                );
            }
        }
    }

    #[test]
    fn cetas_options_preserve_documented_platform_shapes() {
        let verdict: serde_json::Value = serde_json::from_str(include_str!(
            "../../../oracle/verdicts/localdb-17.0.1000.7.surface-fix-cetas-options.json"
        ))
        .unwrap();
        for line in include_str!("../../../oracle/probes/surface-fix-cetas-options.jsonl").lines() {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            let id = case["id"].as_str().unwrap();
            let sql = case["source"].as_str().unwrap();
            let invalid = id.ends_with("/inline-format-options") || id.ends_with("/unknown-option");
            assert!(!parse(sql).root_node().has_error(), "{id}");
            assert_eq!(rules(sql).is_empty(), !invalid, "{id}: {:?}", rules(sql));
            if invalid {
                assert!(
                    verdict["verdicts"][id]
                        .as_object()
                        .unwrap()
                        .values()
                        .all(|v| v.as_array().is_some_and(|a| a == &[serde_json::json!(102)])),
                    "{id}"
                );
            } else {
                assert!(
                    verdict["verdicts"][id]
                        .as_object()
                        .unwrap()
                        .values()
                        .all(|v| v.as_array().is_some_and(|a| a.is_empty()
                            || a == &[serde_json::json!(46553)]
                            || id.ends_with("/mi-partition"))),
                    "{id}"
                );
            }
        }
    }

    #[test]
    fn gap_forms_match_engine_evidence() {
        let verdict: serde_json::Value = serde_json::from_str(include_str!(
            "../../../oracle/verdicts/localdb-17.0.1000.7.gap-forms.json"
        ))
        .unwrap();
        for line in include_str!("../../../oracle/probes/gap-forms.jsonl").lines() {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            let id = case["id"].as_str().unwrap();
            let sql = case["source"].as_str().unwrap();
            let positive = verdict["verdicts"][id]
                .as_object()
                .unwrap()
                .values()
                .any(|v| v.as_array().is_some_and(|a| a.is_empty()));
            let clean = !parse(sql).root_node().has_error() && rules(sql).is_empty();
            // These rejections need declarations, preview/edition settings or catalog state.
            let external = matches!(
                id,
                "gap-forms/original/ddl-rest/commit-off"
                    | "gap-forms/original/kit-routes/alt-tsql-V22-r018"
                    | "gap-forms/vector/aliased"
                    | "gap-forms/vector/no-alias"
                    | "gap-forms/property/scalar"
                    | "gap-forms/external/audit-operator"
                    | "gap-forms/rowset/bulk-ordinal"
                    | "gap-forms/commit/variant-341"
                    | "gap-forms/commit/variant-343"
            );
            assert_eq!(clean, positive || external, "{id}: {sql}");
        }
    }

    #[test]
    fn last_forms_match_engine_evidence() {
        let verdict: serde_json::Value = serde_json::from_str(include_str!(
            "../../../oracle/verdicts/localdb-17.0.1000.7.last-forms.json"
        ))
        .unwrap();
        for line in include_str!("../../../oracle/probes/last-forms.jsonl").lines() {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            let id = case["id"].as_str().unwrap();
            let sql = case["source"].as_str().unwrap();
            let positive = verdict["verdicts"][id]
                .as_object()
                .unwrap()
                .values()
                .any(|v| v.as_array().is_some_and(|a| a.is_empty()));
            let clean = !parse(sql).root_node().has_error() && rules(sql).is_empty();
            assert_eq!(clean, positive, "{id}: {sql}");
        }
        for (sql, expected) in [
            (
                "SELECT 1 WHERE x = 'a' ESCAPE '!';",
                "expression-like-escape",
            ),
            (
                "SELECT JSON_OBJECTAGG('k':x ORDER BY y) FROM t;",
                "expression-json-aggregate",
            ),
            (
                "SELECT * FROM (SELECT 1 UNION SELECT 2 INTO #t) d;",
                "query-select-into-scope",
            ),
        ] {
            assert_eq!(rules(sql), [expected], "{sql}");
        }
    }

    #[test]
    fn ddl_rest_routes_match_engine_boundaries() {
        assert_eq!(
            rules("SELECT 1 FROM a,e,f,b WHERE MATCH(a-(e|f)->b);"),
            ["ddl-rest-graph-edge"]
        );
        for (sql, rule) in [
            (
                "SELECT ALL DISTINCT c FROM dbo.t;",
                "query-select-quantifier",
            ),
            (
                "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){1,0}));",
                "ddl-rest-graph-bound",
            ),
            (
                "SELECT SUM(v) IGNORE NULLS OVER(ORDER BY id) FROM t;",
                "ddl-rest-nulls-modifier",
            ),
            (
                "SELECT ROW_NUMBER() RESPECT NULLS OVER(ORDER BY id) FROM t;",
                "ddl-rest-nulls-modifier",
            ),
            (
                "SELECT 1 FROM a,e,b WHERE MATCH(a-(e)->b,b<-(e)-a);",
                "ddl-rest-graph-match",
            ),
            (
                "SELECT 1 FROM a,e,b WHERE MATCH(SHORTEST_PATH(a-(e)->b+));",
                "ddl-rest-graph-shortest",
            ),
            (
                "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){0,3}));",
                "ddl-rest-graph-bound",
            ),
            (
                "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){2,3}));",
                "ddl-rest-graph-bound",
            ),
        ] {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_mssql::LANGUAGE.into())
                .unwrap();
            assert!(
                !parser.parse(sql, None).unwrap().root_node().has_error(),
                "{sql}: checked by constraints"
            );
            assert_eq!(rules(sql), [rule], "{sql}");
        }
        for sql in [
            "SELECT FIRST_VALUE(v) IGNORE NULLS OVER(ORDER BY id) FROM t;",
            "SELECT LAST_VALUE(v) RESPECT NULLS OVER(ORDER BY id) FROM t;",
            "SELECT LAG(v) IGNORE NULLS OVER(ORDER BY id) FROM t;",
            "SELECT LEAD(v) RESPECT NULLS OVER(ORDER BY id) FROM t;",
            "SELECT 1 FROM a,e,b WHERE MATCH(a-(e)->b AND b<-(e)-a);",
            "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){1,3}));",
            "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH((a<-(e)-)+b));",
            "dbo.p; SELECT 1;",
            "SELECT 1;\nGO\ndbo.p;",
            "/* 한글 C:\\work */ dbo.p 1;",
        ] {
            assert!(rules(sql).is_empty(), "{sql}");
        }
        for sql in ["SELECT 1; dbo.p;", ";dbo.p;", "start: dbo.p;"] {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_mssql::LANGUAGE.into())
                .unwrap();
            let tree = parser.parse(sql, None).unwrap();
            assert!(
                tree.root_node().has_error() || rules(sql) == ["ddl-rest-implicit-exec"],
                "{sql}"
            );
        }
    }

    #[test]
    fn rest_create_constraints_follow_engine_neighbours() {
        for (sql, expected) in [
            (
                "CREATE AGGREGATE dbo.a () RETURNS int EXTERNAL NAME asm.[Ns.Class];",
                "ddl-rest-aggregate",
            ),
            (
                "CREATE AGGREGATE dbo.a (@v int = 1) RETURNS int EXTERNAL NAME asm.[Ns.Class];",
                "ddl-rest-aggregate",
            ),
            (
                "CREATE CRYPTOGRAPHIC PROVIDER p FROM FILE = 0x1234;",
                "ddl-rest-provider-file",
            ),
            (
                "CREATE ASYMMETRIC KEY k FROM EXECUTABLE = 'C:\\a.dll';",
                "ddl-rest-key-source",
            ),
            (
                "CREATE EVENT SESSION e ON SERVER;",
                "ddl-rest-event-session",
            ),
            (
                "CREATE INDEX i ON dbo.t(c) USING GEOMETRY_GRID;",
                "ddl-rest-index-form",
            ),
            (
                "CREATE JSON INDEX i ON dbo.t(j) FOR (p = '$.a');",
                "ddl-rest-index-form",
            ),
            ("CREATE JSON INDEX i ON dbo.t(a,b);", "ddl-rest-index-form"),
            (
                "CREATE SPATIAL INDEX i ON dbo.t(a,b);",
                "ddl-rest-index-form",
            ),
            (
                "CREATE XML INDEX i ON dbo.t(x) USING XML INDEX p FOR WRONG;",
                "ddl-rest-index-form",
            ),
            (
                "CREATE SELECTIVE XML INDEX i ON dbo.t(x);",
                "ddl-rest-index-form",
            ),
            (
                "CREATE PRIMARY XML INDEX i ON dbo.t(x) USING XML INDEX p;",
                "ddl-rest-index-form",
            ),
            (
                "CREATE INDEX i ON dbo.t(c) FOR ('$.a');",
                "ddl-rest-index-form",
            ),
            (
                "CREATE UNIQUE JSON INDEX i ON dbo.t(j);",
                "ddl-rest-index-form",
            ),
        ] {
            assert!(
                parse(sql).root_node().has_error() || rules(sql).contains(&expected),
                "{sql}: {expected}"
            );
        }
        for sql in [
            "CREATE AGGREGATE dbo.a (@v int) RETURNS int EXTERNAL NAME asm.[Ns.Class];",
            "CREATE CRYPTOGRAPHIC PROVIDER p FROM FILE = N'C:\\provider.dll';",
            "SET QUOTED_IDENTIFIER OFF; CREATE CRYPTOGRAPHIC PROVIDER p FROM FILE = \"provider.dll\";",
            "CREATE ASYMMETRIC KEY k FROM EXECUTABLE FILE = 'C:\\a.dll';",
            "CREATE EVENT SESSION e ON DATABASE ADD EVENT sqlserver.error_reported WITH (MAX_DURATION = 2 MINUTES);",
            "CREATE JSON INDEX i ON dbo.t(j) FOR (N'$.a', N'$.b');",
            "CREATE SELECTIVE XML INDEX i ON dbo.t(x) FOR (p = '/a' AS XQUERY 'xs:string');",
            "CREATE PRIMARY XML INDEX i ON dbo.t(x);",
            "CREATE XML INDEX i ON dbo.t(x) USING XML INDEX p FOR PATH;",
            "CREATE XML INDEX i ON dbo.t(x) USING XML INDEX p FOR (a);",
            "CREATE SPATIAL INDEX i ON dbo.t(g) USING BOGUS;",
            "CREATE SPATIAL INDEX i ON dbo.t(g) USING GEOMETRY_GRID WITH (BOUNDING_BOX = (0,0,10));",
            "CREATE APPLICATION ROLE a WITH DEFAULT_SCHEMA = dbo;",
        ] {
            assert!(rules(sql).is_empty(), "{sql}: {:?}", rules(sql));
        }
    }

    #[test]
    fn ddl_rest_alter_neighbours_match_the_engine() {
        for (sql, rule) in [
            (
                "ALTER CREDENTIAL c WITH SECRET = 'x';",
                "ddl-rest-credential",
            ),
            (
                "ALTER DATABASE SCOPED CREDENTIAL c WITH SECRET = 'x';",
                "ddl-rest-credential",
            ),
            (
                "CREATE CREDENTIAL c WITH SECRET = 'placeholder';",
                "ddl-rest-credential",
            ),
            (
                "CREATE REMOTE SERVICE BINDING b TO SERVICE 's' WITH ANONYMOUS = OFF;",
                "ddl-rest-remote-binding",
            ),
            (
                "CREATE RESOURCE POOL p WITH (AFFINITY NUMANODE = AUTO);",
                "ddl-rest-resource-pool",
            ),
            (
                "CREATE RESOURCE POOL p WITH (MAX_CPU_PERCENT = 101);",
                "ddl-rest-resource-pool",
            ),
            (
                "CREATE WORKLOAD GROUP g WITH (IMPORTANCE = WRONG);",
                "ddl-rest-workload-group",
            ),
            (
                "CREATE LOGIN l WITH PASSWORD = 'dummy' OLD_PASSWORD = 'other';",
                "ddl-rest-principal",
            ),
            (
                "ALTER QUEUE q REORGANIZE WITH LOB_COMPACTION = OFF;",
                "ddl-rest-queue",
            ),
            (
                "ALTER MESSAGE TYPE m VALIDATION = INVALID;",
                "ddl-rest-message-type",
            ),
            (
                "ALTER MESSAGE TYPE m VALIDATION = NONE WITH SCHEMA COLLECTION dbo.x;",
                "ddl-rest-message-type",
            ),
            (
                "ALTER AUTHORIZATION ON ASYMMETRIC::k TO dbo;",
                "ddl-rest-authorization",
            ),
        ] {
            assert!(
                parse(sql).root_node().has_error() || rules(sql).contains(&rule),
                "{sql}: {rule}"
            );
        }
        for sql in [
            "ALTER REMOTE SERVICE BINDING b WITH ANONYMOUS = OFF;",
            "CREATE APPLICATION ROLE a WITH DEFAULT_SCHEMA = dbo;",
            "CREATE CREDENTIAL c WITH IDENTITY = 'dummy';",
            "ALTER CREDENTIAL c WITH IDENTITY = 'dummy', SECRET = 'placeholder';",
            "CREATE REMOTE SERVICE BINDING b TO SERVICE 's' WITH USER = u, ANONYMOUS = OFF;",
            "CREATE RESOURCE POOL p WITH (AFFINITY SCHEDULER = AUTO, MAX_CPU_PERCENT = 50);",
            "ALTER RESOURCE POOL p WITH (AFFINITY NUMANODE = (0, 1 TO 2));",
            "CREATE WORKLOAD GROUP g WITH (IMPORTANCE = MEDIUM) USING p;",
            "ALTER WORKLOAD GROUP g WITH (IMPORTANCE = HIGH);",
            "CREATE BROKER PRIORITY p FOR CONVERSATION SET (PRIORITY_LEVEL = 11);",
            "ALTER LOGIN l WITH PASSWORD = 'dummy' OLD_PASSWORD = 'other';",
            "CREATE LOGIN l WITH PASSWORD = 'dummy' MUST_CHANGE, CHECK_POLICY = ON;",
            "ALTER QUEUE q REORGANIZE WITH (LOB_COMPACTION = OFF);",
            "ALTER MESSAGE TYPE m VALIDATION = VALID_XML WITH SCHEMA COLLECTION dbo.x;",
            "ALTER AUTHORIZATION ON ASYMMETRIC KEY::k TO dbo;",
        ] {
            assert!(rules(sql).is_empty(), "{sql}: {:?}", rules(sql));
        }
    }

    #[test]
    fn ddl_rest_permission_tails_match_the_engine() {
        for sql in [
            "GRANT SELECT ON dbo.t FROM u;",
            "DENY SELECT ON dbo.t FROM u;",
            "GRANT SELECT ON dbo.t TO u CASCADE;",
            "DENY SELECT ON dbo.t TO u WITH GRANT OPTION;",
            "REVOKE SELECT ON dbo.t FROM u WITH GRANT OPTION;",
            "GRANT GRANT OPTION FOR SELECT ON dbo.t TO u;",
            "DENY GRANT OPTION FOR SELECT ON dbo.t TO u;",
        ] {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_mssql::LANGUAGE.into())
                .unwrap();
            assert!(
                !parser.parse(sql, None).unwrap().root_node().has_error(),
                "{sql}: this boundary is enforced by constraints"
            );
            assert_eq!(rules(sql), ["ddl-rest-permission"], "{sql}");
        }
        for sql in [
            "GRANT SELECT ON dbo.t TO u WITH GRANT OPTION AS dbo;",
            "DENY SELECT ON dbo.t TO u CASCADE AS dbo;",
            "REVOKE SELECT ON dbo.t TO u;",
            "REVOKE SELECT ON dbo.t FROM u CASCADE AS dbo;",
            "REVOKE GRANT OPTION FOR SELECT ON dbo.t TO u CASCADE AS dbo;",
            r"GRANT CONTROL ON ASYMMETRIC/* 한국어 C:\keys */KEY::ak TO u;",
        ] {
            assert!(rules(sql).is_empty(), "{sql}");
        }
    }

    #[test]
    fn expression_constraints_match_engine_neighbours() {
        let cases: Vec<serde_json::Value> = include_str!("../../../oracle/probes/expr-forms.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        for (id, expected) in [
            ("language/contains-float", "expression-fulltext-language"),
            ("language/contains-repeat", "expression-fulltext-language"),
            ("language/ordinary", "expression-fulltext-language"),
            (
                "language/containstable-top-before",
                "expression-fulltext-language",
            ),
            (
                "language/containstable-top-hex",
                "expression-fulltext-language",
            ),
            ("static/database", "expression-static-reference"),
            ("static/extra-methodVariable", "expression-static-reference"),
            ("static/nullCall", "expression-static-reference"),
            ("set/innerTop", "expression-nested-set-clause"),
            ("provider/numbers", "expression-rowset-provider"),
            ("provider/last", "expression-rowset-provider"),
            ("provider/schema", "expression-rowset-provider"),
        ] {
            let case = cases
                .iter()
                .find(|c| c["id"] == format!("expr-forms/{id}"))
                .unwrap();
            assert_eq!(rules(case["source"].as_str().unwrap()), [expected], "{id}");
        }
        for id in [
            "language/contains-name",
            "language/contains-declared",
            "language/containstable-top-declared",
            "language/qi-comment",
            "language/qi-options",
            "static/schema",
            "static/nullQuoted",
            "set/deep",
            "provider/omitted",
            "provider/table",
        ] {
            let case = cases
                .iter()
                .find(|c| c["id"] == format!("expr-forms/{id}"))
                .unwrap();
            assert!(rules(case["source"].as_str().unwrap()).is_empty(), "{id}");
        }
    }

    #[test]
    fn spaced_comparisons_keep_the_operator_kinds() {
        for op in ["!=", "<=", ">=", "<>", "!<", "!>"] {
            for gap in [
                " ",
                "\r\n",
                "-- 한글\n",
                "/* a /* b */ c */",
                "\u{200b}/*c*/",
            ] {
                let sql = format!("SELECT 1 WHERE 1 {}{gap}{} 2;", &op[..1], &op[1..]);
                let mut parser = Parser::new();
                parser
                    .set_language(&tree_sitter_mssql::LANGUAGE.into())
                    .unwrap();
                let tree = parser.parse(&sql, None).unwrap();
                assert!(!tree.root_node().has_error(), "{sql}");
                let mut nodes = vec![tree.root_node()];
                let mut found = false;
                while let Some(node) = nodes.pop() {
                    if let Some(operator) = node.child_by_field_name("operator") {
                        assert_eq!(operator.kind(), op, "{sql}");
                        assert!(!operator.is_named());
                        found = true;
                        break;
                    }
                    let mut cursor = node.walk();
                    nodes.extend(node.named_children(&mut cursor));
                }
                assert!(found, "{sql}");
            }
        }
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
    fn conditions_are_not_scalars() {
        for ok in [
            "SELECT a FROM t JOIN u ON t.a = u.a WHERE a IS NOT NULL AND b NOT LIKE 'x' OR NOT (c IN (1));",
            "SELECT a FROM t GROUP BY a HAVING COUNT(*) > 1;",
            "SELECT CASE WHEN a = 1 THEN 1 END, CASE a WHEN 1 THEN 2 END, IIF(a > 1, 1, 0) FROM t;",
            "SELECT 1 WHERE EXISTS (SELECT 1) AND 1 BETWEEN 0 AND 2 AND a IS DISTINCT FROM b;",
            "IF (@x = 1) PRINT 1;",
            "WHILE NOT (1 = 0) BREAK;",
            "MERGE dbo.t USING dbo.s ON (t.id = s.id) WHEN MATCHED AND s.a > 0 THEN DELETE;",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for bad in [
            "IF 1 PRINT 1;",
            "IF NOT 1 PRINT 1;",
            "WHILE @x BREAK;",
            "SELECT a FROM t WHERE a;",
            "SELECT a FROM t WHERE a = 1 AND b;",
            "SELECT a FROM t GROUP BY a HAVING COUNT(*);",
            "SELECT CASE WHEN a THEN 1 END FROM t;",
            "SELECT IIF(a, 1, 0) FROM t;",
            "MERGE dbo.t USING dbo.s ON t.id + 1 WHEN MATCHED THEN DELETE;",
            "MERGE dbo.t USING dbo.s ON (t.id) WHEN MATCHED THEN DELETE;",
        ] {
            assert_eq!(rules(bad), ["scalar-predicate"], "{bad}");
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

    #[test]
    fn query_forms_constraints_match_engine_boundaries() {
        let cases: Vec<serde_json::Value> =
            include_str!("../../../oracle/probes/query-forms.jsonl")
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
        let source = |id: &str| {
            cases
                .iter()
                .find(|c| c["id"] == format!("query-forms/{id}"))
                .unwrap()["source"]
                .as_str()
                .unwrap()
        };
        for (id, expected) in [
            ("select-distinct-all", "query-select-quantifier"),
            ("function-cursor-param", "query-cursor-parameter"),
            ("function-native-returns-null", "query-native-null-input"),
            ("offsets-bogus", "query-offsets-options"),
            ("cast-using", "query-cast-using"),
            ("try-cast-using", "query-cast-using"),
            ("cursor-iso", "query-cursor-options"),
            ("cursor-iso-after", "query-cursor-options"),
            ("cursor-bogus", "query-cursor-options"),
            ("cursor-two-scope", "query-cursor-options"),
            ("cursor-fast-scroll", "query-cursor-options"),
            ("cursor-read-optimistic", "query-cursor-options"),
            ("cursor-static-keyset", "query-cursor-options"),
            ("update-column-chain", "query-assignment-chain"),
            ("select-delete-source", "query-dml-source"),
            ("insert-delete-no-output", "query-dml-source"),
            ("openjson-not-null", "query-rowset-schema"),
            ("openxml-not-null", "query-rowset-schema"),
            ("openjson-table-schema", "query-rowset-schema"),
            ("exec-repeat-recompile", "query-exec-options"),
            ("exec-repeat-results", "query-exec-options"),
            ("plain-atomic", "query-atomic-options"),
            ("function-native-missing-atomic", "query-native-atomic"),
            ("atomic-no-language", "query-atomic-options"),
            ("atomic-no-isolation", "query-atomic-options"),
            ("atomic-repeat-language", "query-atomic-options"),
            ("atomic-repeat-isolation", "query-atomic-options"),
            ("atomic-unknown", "query-atomic-options"),
            ("atomic-datefirst-var", "query-atomic-options"),
            ("atomic-language-var", "query-atomic-options"),
            ("atomic-durability-bad", "query-atomic-options"),
            ("atomic-language-no-equals", "query-atomic-options"),
        ] {
            let sql = source(id);
            let mut p = Parser::new();
            p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
            assert!(
                !p.parse(sql, None).unwrap().root_node().has_error(),
                "{id}: this boundary is enforced by constraints"
            );
            assert_eq!(rules(sql), [expected], "{id}");
        }
        for id in [
            "offsets-valid",
            "offsets-repeat",
            "cursor-iso-reverse",
            "cursor-iso-repeat",
            "cursor-set-repeat",
            "cursor-duplicate-local",
            "cursor-global-comment",
            "cursor-comment-literal",
            "exec-repeat-comment",
            "predict-schema",
            "predict-collate",
            "atomic-all",
            "atomic-datefirst-eight",
            "atomic-format-bad",
            "atomic-language-literal-keyword",
            "parse-using",
            "try-parse",
            "update-variable-chain",
            "update-compound-chain",
            "insert-delete-output",
            "insert-update-output",
            "insert-insert-output",
            "procedure-native",
        ] {
            assert!(rules(source(id)).is_empty(), "{id}");
        }
    }

    #[test]
    fn backup_restore_options_match_the_engine() {
        for ok in [
            "BACKUP DATABASE d FILE = 'f', FILEGROUP = @g, READ_WRITE_FILEGROUPS TO DISK = 'a', @dev MIRROR TO URL = N'u' WITH FORMAT, STATS, STATS = -1, NAME = N'n', COMPRESSION (ALGORITHM = ZSTD, LEVEL = HIGH), ENCRYPTION (ALGORITHM = AES_256, SERVER CERTIFICATE = c);",
            "BACKUP GROUP a, [b] TO DISK = 'a' WITH METADATA_ONLY;",
            "RESTORE LOG d PAGE = '1:57' FROM dev, DATABASE_SNAPSHOT = 's' WITH FILE = 2, MOVE N'a' TO @b, STOPATMARK = 'm' AFTER @t, FILESTREAM (DIRECTORY_NAME = 'x'), RECOVERY, NORECOVERY;",
            "RESTORE HEADERONLY FROM VIRTUAL_DEVICE = 'v' WITH PARTIAL, FILE = @f;",
            "BACKUP CERTIFICATE c TO FILE = 'a' WITH PRIVATE KEY (ENCRYPTION BY PASSWORD = 'p'), FORMAT = 'PFX', PRIVATE KEY (FILE = 'b', ALGORITHM = 'AES_256');",
            "BACKUP MASTER KEY TO URL = 'u' ENCRYPTION BY PASSWORD = 'p';",
        ] {
            assert!(rules(ok).is_empty(), "{ok}");
        }
        for (bad, rule) in [
            (
                "BACKUP DATABASE d FOO = 'f' TO DISK = 'a';",
                "backup-restore-file",
            ),
            (
                "BACKUP DATABASE d FILE = 1 TO DISK = 'a';",
                "backup-restore-file",
            ),
            (
                "RESTORE LOG d READ_WRITE_FILEGROUPS FROM DISK = 'a';",
                "backup-restore-file",
            ),
            ("BACKUP DATABASE d TO PIPE = 'a';", "backup-restore-device"),
            ("BACKUP DATABASE d TO DISK 'a';", "backup-restore-device"),
            (
                "RESTORE DATABASE d FROM DISK = foo;",
                "backup-restore-device",
            ),
            ("BACKUP GROUP @a TO DISK = 'a';", "backup-restore-device"),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH FOO;",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH REPLACE;",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH INIT = 1;",
                "backup-restore-option",
            ),
            (
                "RESTORE DATABASE d FROM DISK = 'a' WITH CREDENTIAL;",
                "backup-restore-option",
            ),
            (
                "RESTORE DATABASE d FROM DISK = 'a' WITH FILE = 'x';",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH NAME = foo;",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH BLOCKSIZE = 0x10;",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH MOVE 'a' TO 'b';",
                "backup-restore-option",
            ),
            (
                "RESTORE DATABASE d FROM DISK = 'a' WITH MOVE 1 TO 2;",
                "backup-restore-option",
            ),
            (
                "RESTORE LOG d FROM DISK = 'a' WITH STOPAT = 'x' AFTER 'y';",
                "backup-restore-option",
            ),
            (
                "RESTORE LOG d FROM DISK = 'a' WITH STOPATMARK = 'm' AFTER 1;",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH COMPRESSION (LEVEL = LOW, ALGORITHM = ZSTD);",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH ENCRYPTION (ALGORITHM = FOO, SERVER CERTIFICATE = c);",
                "backup-restore-option",
            ),
            (
                "BACKUP DATABASE d TO DISK = 'a' WITH ENCRYPTION (ALGORITHM = AES_256, SERVER CERTIFICATE = @c);",
                "backup-restore-option",
            ),
            (
                "RESTORE DATABASE d FROM DISK = 'a' WITH FILESTREAM (DIRECTORY_NAME = @v);",
                "backup-restore-option",
            ),
            (
                "RESTORE DATABASE d FROM DISK = 'a' WITH COMPRESSION;",
                "backup-restore-option",
            ),
            (
                "BACKUP CERTIFICATE c TO FILE = 'a' WITH FORMAT = 'PFX', FORMAT = 'PFX';",
                "backup-certificate-options",
            ),
            (
                "BACKUP CERTIFICATE c TO FILE = 'a' WITH PRIVATE KEY (ENCRYPTION BY PASSWORD = 'p');",
                "backup-certificate-options",
            ),
            (
                "BACKUP CERTIFICATE c TO FILE = 'a' WITH PRIVATE KEY (FILE = 'b', ENCRYPTION BY PASSWORD = @p);",
                "backup-certificate-options",
            ),
            (
                "BACKUP CERTIFICATE c TO FILE = 'a' WITH FOO = 'x';",
                "backup-certificate-options",
            ),
        ] {
            assert_eq!(rules(bad), [rule], "{bad}");
        }
    }
    #[test]
    fn ddl_rest_misc_matches_engine_neighbours() {
        for bad in [
            "COMMIT WITH (WHATEVER = ON);",
            "COMMIT WITH (DELAYED_DURABILITY = ON, DELAYED_DURABILITY = OFF);",
            "COMMIT WITH (DELAYED_DURABILITY = 1);",
            "COMMIT WITH (DELAYED_DURABILITY);",
            "KILL QUERY NOTIFICATION SUBSCRIPTION 'x';",
            "UPDATE STATISTICS dbo.t WITH FULLSCAN ON PARTITIONS (1);",
            "UPDATE STATISTICS dbo.t WITH FULLSCAN, COLUMNS, INDEX;",
            "UPDATE STATISTICS dbo.t WITH WHATEVER = 1;",
            "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (RANK = WRONG);",
            "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (LABEL = 1);",
            "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (LABEL = 'a', LABEL = 'b');",
            "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (WHATEVER = 'a');",
            "DROP SENSITIVITY CLASSIFICATION FROM c;",
            "ADD SIGNATURE TO dbo.p BY CERTIFICATE c WITH WHATEVER = 1;",
            "ADD SIGNATURE TO dbo.p BY SYMMETRIC KEY k WITH PASSWORD = 'x';",
        ] {
            assert!(!parse(bad).root_node().has_error(), "{bad}");
            assert_eq!(rules(bad), ["ddl-rest-misc"], "{bad}");
        }
        for good in [
            "COMMIT;",
            "COMMIT TRAN t WITH (DELAYED_DURABILITY = ON);",
            "COMMIT WORK WITH (DELAYED_DURABILITY = OFF);",
            "KILL QUERY NOTIFICATION SUBSCRIPTION ALL;",
            "KILL STATS JOB -1;",
            "UPDATE STATISTICS dbo.t WITH RESAMPLE ON PARTITIONS (1, 3 TO 5), COLUMNS;",
            "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (LABEL = N'기밀', RANK = HIGH);",
            "DROP SENSITIVITY CLASSIFICATION FROM dbo.t.c;",
            "ADD SIGNATURE TO dbo.p BY CERTIFICATE c WITH SIGNATURE = 0x0102;",
            "ADD SIGNATURE TO dbo.p BY SYMMETRIC KEY k;",
        ] {
            assert!(!parse(good).root_node().has_error(), "{good}");
            assert!(rules(good).is_empty(), "{good}");
        }
    }
    #[test]
    fn ddl_rest_closing_neighbours_match_the_engine() {
        for (sql, rule) in [
            (
                "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE (sqlserver.database_id) < 8);",
                "ddl-rest-xe-source",
            ),
            (
                "ALTER EVENT SESSION es ON SERVER ADD EVENT sqlserver.sql_statement_completed (WHERE (duration) < 8);",
                "ddl-rest-xe-source",
            ),
            (
                "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE sqlserver.equal_i_sql_unicode_string((sqlserver.client_app_name), N'x'));",
                "ddl-rest-xe-source",
            ),
            ("DROP SEQUENCE db1.dbo.seq_a;", "ddl-rest-drop-sequence"),
            ("DROP SEQUENCE db1..seq_a;", "ddl-rest-drop-sequence"),
            ("DROP SEQUENCE srv.db1.dbo.seq_a;", "ddl-rest-drop-sequence"),
            (
                "DROP SEQUENCE dbo.seq_a, db1.dbo.seq_b;",
                "ddl-rest-drop-sequence",
            ),
            (
                "DROP SEQUENCE IF EXISTS db1.dbo.seq_a;",
                "ddl-rest-drop-sequence",
            ),
            (
                "REVOKE CONNECT ON AVAILABILITY GROUP::ag FROM u;",
                "ddl-rest-ag-connect",
            ),
            (
                "REVOKE CONNECT ON AVAILABILITY GROUP::ag TO u;",
                "ddl-rest-ag-connect",
            ),
            (
                "GRANT CONNECT ON AVAILABILITY GROUP::ag TO u;",
                "ddl-rest-ag-connect",
            ),
            (
                "DENY CONNECT ON AVAILABILITY GROUP::ag TO u;",
                "ddl-rest-ag-connect",
            ),
            (
                "GRANT CONTROL ON [ASYMMETRIC KEY]::ak TO u;",
                "ddl-rest-securable-class",
            ),
            (
                r"REVOKE ALTER ON [AVAILABILITY GROUP]::[ag_sales] TO [CONTOSO\Mary];",
                "ddl-rest-securable-class",
            ),
        ] {
            let tree = parse(sql);
            assert!(
                !tree.root_node().has_error(),
                "{sql}: this boundary is enforced by constraints"
            );
            assert_eq!(rules(sql), [rule], "{sql}");
        }
        for sql in [
            "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE sqlserver.database_id < 8);",
            "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE ((sqlserver.database_id < 8)));",
            "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE (sqlserver.database_id < 8) AND NOT (duration > 5));",
            "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE sqlserver.equal_i_sql_unicode_string(sqlserver.client_app_name, N'x'));",
            "DROP SEQUENCE dbo.seq_a, seq_b;",
            "DROP SEQUENCE IF EXISTS dbo.seq_a;",
            "REVOKE ALTER ON AVAILABILITY GROUP::ag TO u;",
            "REVOKE CONTROL ON AVAILABILITY GROUP::ag FROM u;",
            "GRANT ALTER ON AVAILABILITY GROUP::ag TO u;",
            "DENY CONTROL ON AVAILABILITY GROUP::ag TO u;",
            r"REVOKE GRANT OPTION FOR CONTROL ON AVAILABILITY GROUP::ag_sales FROM l_app, [CONTOSO\Mary], l_cert, l_akey CASCADE AS l_admin;",
            r"REVOKE ALTER ON AVAILABILITY/* 한국어 C:\audit */GROUP::[ag_sales] TO [CONTOSO\Mary];",
        ] {
            let tree = parse(sql);
            assert!(!tree.root_node().has_error(), "{sql}");
            assert!(rules(sql).is_empty(), "{sql}");
        }
    }
}
