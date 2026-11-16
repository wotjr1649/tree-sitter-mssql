use super::Violation;
use super::expr::*;
use super::hints::*;
use super::statements::*;
use super::util::*;
use crate::tree::{has_child, named_children};
use tree_sitter::Node;

/// R20 gap-forms, LocalDB 17.0.1000.7, levels 100-170: tree-local call boundaries
/// (102/156/174). Qualified user functions are not built-ins; write targets take constants
/// or local variables, not general expressions. Keep the tree for queries and consumers.
pub(super) fn gap_invocation(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let args: Vec<_> = node.children_by_field_name("parameter", &mut c).collect();
    let name = invocation_name(node, source).unwrap_or_default();

    let target = node
        .parent()
        .is_some_and(|p| matches!(p.kind(), "delete" | "update"));
    let invalid = match name.as_str() {
        "CONVERT" => args
            .first()
            .is_some_and(|a| string_literal(a.child_by_field_name("value").unwrap_or(*a), source)),
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
                        || v.syntax_named_child_count() != 1
                        || v.syntax_named_child(0).is_none_or(|n| {
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
                    .and_then(|p| p.syntax_next_named_sibling())
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

pub(super) fn gap_constant(node: Node, source: &[u8]) -> bool {
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
pub(super) fn gap_scalar(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "exec_argument" => node
            .child_by_field_name("value")
            .is_some_and(|n| n.kind() == "unary_expression" && has_child(n, "+")),
        "term" => node.child_by_field_name("value").is_some_and(|n| {
            // A subquery is scalar; avoid a root-to-node parent lookup at every nesting level.
            n.kind() != "subquery"
                && node
                    .parent()
                    .is_some_and(|p| p.kind() == "select_expression")
                && is_condition(n, source)
                && !(n.kind() == "binary_expression"
                    && n.child_by_field_name("operator")
                        .is_some_and(|o| o.kind() == "=")
                    && n.child_by_field_name("left").is_some_and(|l| {
                        string_literal(l, source)
                            || l.kind() == "field"
                                && l.syntax_named_child_count() == 1
                                && l.syntax_named_child(0)
                                    .is_some_and(|id| id.kind() == "identifier")
                    }))
        }),
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

/// R20 name/qualification, engine 117/156: XML collections have at most schema.name,
/// HISTORY_TABLE has exactly schema.name; XE qualified names have no empty components.
pub(super) fn gap_name(node: Node, source: &[u8], out: &mut Vec<Violation>) {
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
                        && parent.syntax_named_child_count() > 1
                        && parent.syntax_named_child(0) != Some(node))
        {
            out.push(violation(
                "gap-reserved-name",
                "this reserved name must be delimited".into(),
                node,
            ));
        }
        return;
    }
    let parts = named_children(node).len();
    let mut c = node.walk();
    let dots = node
        .syntax_children(&mut c)
        .filter(|n| n.kind() == ".")
        .count();
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
            .syntax_prev_named_sibling()
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
pub(super) fn gap_options(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "table_hint" => {
            let target = dml_target(node);
            let merge_source =
                node.parent().is_some_and(|n| n.kind() == "merge") && target.is_none();
            merge_source
                || !has_child(node, "keyword_with")
                    && (target.is_some() || {
                        named_children(node)
                            .into_iter()
                            .any(|n| n.kind() == "table_hint_item" && has_child(n, "keyword_index"))
                    })
        }
        "query_hint" => {
            let parts: Vec<_> = named_children(node);
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
                    .syntax_prev_named_sibling()
                    .is_some_and(|n| n.kind() == "keyword_with")
                && index.is_some()
                && (!matches!(name.as_str(), "DATA_COMPRESSION" | "XML_COMPRESSION")
                    || index.is_some_and(|i| {
                        let parts: Vec<_> = named_children(i);
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
pub(super) fn gap_statement(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "block" | "try_block" => !named_children(node)
            .into_iter()
            .any(|n| matches!(n.kind(), "statement" | "block" | "label_statement")),
        "enable_trigger_statement" => node.parent().is_some_and(|s| {
            let next = s.syntax_next_sibling();
            next.is_some_and(|n| n.kind() != ";" && n.kind() != "go_statement")
        }),
        "window_frame" => {
            !has_child(node, "keyword_between")
                && node.syntax_named_child(1).is_some_and(|f| {
                    has_child(f, "keyword_unbounded") && has_child(f, "keyword_following")
                })
        }
        "create_event_session" => !has_child(node, "event_session_event"),
        "alter_event_session" => named_children(node).len() == 6,
        "create_route" | "alter_route" => {
            let options = named_children(node)
                .into_iter()
                .find(|n| n.kind() == "with_clause")
                .map(direct_options)
                .unwrap_or_default();
            let mut seen = std::collections::BTreeSet::new();
            let mut duplicate = false;
            for opt in &options {
                let name = option_name(*opt, source);
                duplicate |= !seen.insert(name);
            }
            duplicate || node.kind() == "create_route" && !seen.iter().any(|s| s == "ADDRESS")
        }
        "alter_database" => {
            let handle = named_children(node)
                .into_iter()
                .find(|n| n.kind() == "keyword_procedure_cache")
                .and_then(|value| value.syntax_next_named_sibling());
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
            let parts: Vec<_> = named_children(node);
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
            let words: Vec<_> = named_children(node)
                .into_iter()
                .filter(|n| n.kind() != "list")
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
pub(super) const DATABASE_TRIGGER_EVENTS: &[&str] = &[
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
