use super::Violation;
use super::gap::gap_options;
use super::util::*;
use crate::tree::{has_child, named_children};
use tree_sitter::Node;

/// A statement that starts with a CTE must not directly follow another statement or block: the
/// previous one needs a `;`. It may come first in a batch, block or module body, right after
/// `;`, a label, or as the body of IF, ELSE or WHILE. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, Msg 319 for the semicolon/*-no-terminator probes (after SELECT,
/// END, IF, IF ... ELSE, WHILE and inside a procedure body), acceptance of every terminated,
/// leading-semicolon, label, IF/ELSE/WHILE-body and first-in-block form.
pub(super) fn cte_terminator(statement: Node, out: &mut Vec<Violation>) {
    // `WITH c AS (...) SELECT ...` is a statement whose children start with `keyword_with`, `cte`.
    let starts_with_cte = statement.syntax_named_child(0).map(|n| n.kind()) == Some("keyword_with")
        && statement.syntax_named_child(1).map(|n| n.kind()) == Some("cte");
    if !starts_with_cte {
        return;
    }
    let Some(prev) = statement.syntax_prev_sibling() else {
        return;
    };
    let follows_statement = match prev.kind() {
        "block" => true,
        "statement" => prev.syntax_named_child(0).map(|n| n.kind()) != Some("label_statement"),
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
pub(super) fn exec_output_requires_at(statement: Node, out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.syntax_children(&mut cursor).collect();
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
pub(super) fn exec_output_variable(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let is_output = |n: &Node| matches!(n.kind(), "keyword_output" | "keyword_out");
    let local_variable = |n: Node| {
        let name = if n.kind() == "field" && n.syntax_named_child_count() == 1 {
            n.child_by_field_name("name")
        } else {
            Some(n)
        };
        name.filter(|n| n.kind() == "identifier")
            .and_then(|n| n.utf8_text(source).ok())
            .is_some_and(|text| text.starts_with('@') && !text.starts_with("@@"))
    };
    let mut cursor = statement.walk();
    for child in statement.syntax_children(&mut cursor).collect::<Vec<_>>() {
        // EXEC ( string, value OUTPUT ) AT: the value is the named node right before OUTPUT.
        if is_output(&child) {
            if !child
                .syntax_prev_named_sibling()
                .is_some_and(local_variable)
            {
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
            let marked = child.syntax_children(&mut c).any(|n| is_output(&n));
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
pub(super) fn column_permissions(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.syntax_children(&mut cursor).collect();
    let on_object = children.iter().any(|n| n.kind() == "(");
    for permission in children.iter().filter(|n| n.kind() == "permission") {
        let parts: Vec<Node> = named_children(*permission);
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
pub(super) const SERVER_ONLY_EVENTS: &[&str] = &[
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

/// The shared procedure body can be bare AS. Triggers need a statement or EXTERNAL NAME.
/// LocalDB 17.0.1000.7, levels 100-170, both QUOTED_IDENTIFIER settings:
/// batch-rules/{create,alter}-trigger-no-body and trigger-semicolon-only (Msg 102).
pub(super) fn trigger_body(trigger: Node, out: &mut Vec<Violation>) {
    if let Some(body) = trigger.child_by_field_name("body")
        && body.syntax_named_child_count() == 1
    {
        out.push(violation(
            "trigger-body",
            "a trigger needs a statement or EXTERNAL NAME after AS".into(),
            body,
        ));
    }
}

/// TVF return shapes select the body form. LocalDB 17.0.1000.7, levels 100-170, both sessions:
/// batch-rules function cases for table-columns (Msg 156), inline-table (156/102) and
/// table-variable (156/102) reject mismatched bodies.
pub(super) fn function_return_body(function: Node, out: &mut Vec<Violation>) {
    let (required, detail) = if has_child(function, "column_definitions") {
        (
            "external_name",
            "RETURNS TABLE with a column list requires an EXTERNAL NAME body",
        )
    } else if has_child(function, "table_var_declaration") {
        (
            "keyword_begin",
            "RETURNS @variable TABLE requires a BEGIN ... END body",
        )
    } else if has_child(function, "keyword_table") {
        (
            "keyword_return",
            "inline RETURNS TABLE requires RETURN followed by a query",
        )
    } else {
        return;
    };
    if let Some(body) = function
        .child_by_field_name("body")
        .filter(|n| !has_child(*n, required))
    {
        out.push(violation("function-return-body", detail.into(), body));
    }
}

/// Trigger scope, decided by `ON DATABASE` / `ON ALL SERVER` (a DDL trigger) versus `ON object`
/// (a DML trigger). Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set
/// `context/trig-*` and the trigger-* cases, for CREATE, ALTER and CREATE OR ALTER:
/// - a DDL trigger name has no schema (Msg 1094);
/// - a DDL trigger takes no INSERT/UPDATE/DELETE and a DML trigger nothing else (Msg 1098);
/// - `ON DATABASE` takes no server-only event (Msg 1098); ALTER_SERVER_CONFIGURATION is
///   rejected on both scopes (Msg 1082);
/// - a DDL trigger cannot `EXECUTE AS OWNER` (Msg 1083).
pub(super) fn trigger_scope(trigger: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = trigger.walk();
    let children: Vec<Node> = trigger.syntax_children(&mut cursor).collect();
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
                && n.syntax_children(&mut c)
                    .any(|k| k.kind() == "keyword_owner")
            {
                out.push(violation(
                    "trigger-scope",
                    "a DATABASE or ALL SERVER trigger cannot EXECUTE AS OWNER".into(),
                    n,
                ));
            }
            stack.extend(named_children(n));
        }
    }
}

/// `WITH c AS (...) SELECT ...` with no table source outside the CTE definitions (no FROM anywhere
/// in the query, subqueries included). A set operation is exempt. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, oracle set `context/cte-*` and cte-none: Msg 422 for a FROM-less
/// SELECT (also with two CTEs); accepted with FROM in the query or a subquery, for UNION, and for
/// INSERT and UPDATE. A WITH XMLNAMESPACES alone is not a CTE; FOR also permits a FROM-less
/// query (surface-fix-constraints, LocalDB 17.0.1000.7, levels 100-170).
pub(super) fn cte_table_source(statement: Node, out: &mut Vec<Violation>) {
    let children: Vec<Node> = named_children(statement);
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
        stack.extend(named_children(n));
    }
    out.push(violation(
        "cte-table-source",
        "a query after WITH needs a table source".into(),
        statement,
    ));
}

pub(super) fn table_variable_foreign_key(columns: Node, out: &mut Vec<Violation>) {
    if !matches!(
        columns.parent().map(|p| p.kind()),
        Some("table_var_declaration" | "create_type")
    ) {
        return;
    }
    let mut stack = vec![columns];
    while let Some(n) = stack.pop() {
        if n.kind() == "keyword_references" {
            out.push(violation(
                "table-variable-foreign-key",
                "a table variable or table type cannot have a foreign key".into(),
                n,
            ));
            return;
        }
        stack.extend(n.children(&mut n.walk()));
    }
}

/// MERGE must end with `;`: the next token after it is a semicolon, wherever the statement sits
/// (batch, block, IF or ELSE branch, procedure body). Evidence: SQL Server 2025 LocalDB 17.0.1000.7,
/// levels 100-170, oracle sets `context/merge-*` and `semicolon/merge-*`: Msg 10713 otherwise; a
/// comment before the semicolon is fine. A MERGE used as a table source (`FROM (MERGE ... OUTPUT
/// ...) AS x`) is not a statement and is not checked (accepted; fixture merge_as_from_source.sql).
pub(super) fn merge_terminator(merge: Node, out: &mut Vec<Violation>) {
    if merge.parent().map(|p| p.kind()) != Some("statement") {
        return;
    }
    let mut n = merge;
    let next = loop {
        match n.syntax_next_sibling() {
            Some(s) => break Some(s),
            None => match n.parent() {
                Some(p) => n = p,
                None => break None,
            },
        }
    };
    // The token itself: the next node may start with it (`ELSE` clause after `IF ... MERGE ...;`).
    let first_token = next.map(|mut n| {
        while let Some(c) = n.syntax_child(0) {
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

pub(super) fn quoted_string_argument(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let string_slot = match node.kind() {
        "execute_statement" => node.child_by_field_name("procedure").is_none(),
        "waitfor_statement" => has_child(node, "keyword_delay") || has_child(node, "keyword_time"),
        _ => true,
    };
    if string_slot
        && let Some(value) = named_children(node)
            .into_iter()
            .find(|n| !n.kind().starts_with("keyword_"))
        && source.get(value.start_byte()) == Some(&b'"')
    {
        out.push(violation(
            "quoted-identifier-string",
            "this argument requires a string, not a double-quoted identifier".into(),
            value,
        ));
    }
}

pub(super) fn collation_name(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    if let Some(name) = node.syntax_next_named_sibling()
        && matches!(source.get(name.start_byte()), Some(b'\'' | b'"' | b'['))
    {
        out.push(violation(
            "collation-name",
            "COLLATE requires an unquoted collation name".into(),
            name,
        ));
    }
}

pub(super) fn bulk_path(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    if let Some(path) = named_children(node)
        .into_iter()
        .find(|n| n.kind() == "literal")
        && empty_string(path, source)
    {
        out.push(violation(
            "bulk-path-empty",
            "BULK INSERT requires a nonempty path".into(),
            path,
        ));
    }
}

pub(super) fn filegroup_name(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    if let Some(name) = named_children(node)
        .into_iter()
        .find(|n| empty_string(*n, source))
    {
        out.push(violation(
            "filegroup-name-empty",
            "this filegroup placement requires a nonempty name".into(),
            name,
        ));
    }
}

pub(super) fn option_gaps(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    gap_options(node, source, out);
    if node.kind() == "option"
        && (ancestor(node, "drop_index").is_some() || ancestor(node, "alter_table").is_some())
        && node
            .child_by_field_name("name")
            .is_some_and(|n| matches!(upper_text(n, source).as_str(), "MOVE" | "FILESTREAM_ON"))
        && let Some(value) = node
            .child_by_field_name("value")
            .filter(|n| empty_string(*n, source))
    {
        out.push(violation(
            "filegroup-name-empty",
            "this filegroup placement requires a nonempty name".into(),
            value,
        ));
    }
}
