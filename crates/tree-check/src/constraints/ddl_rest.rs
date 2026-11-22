use super::Violation;
use super::backup_restore::*;
use super::util::*;
use crate::tree::{ancestors, has_child, named_children};
use tree_sitter::Node;

/// Permission tails are verb-specific. SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170,
/// ddl-rest/permission-{grant-from,deny-from,grant-cascade,deny-with-grant,revoke-with-grant}
/// reject with Msg 102; permission-{grant,deny}-grant-option-for reject with Msg 156/102.
/// REVOKE accepts TO and FROM, with or without GRANT OPTION FOR, and CASCADE/AS.
pub(super) fn ddl_rest_permissions(statement: Node, out: &mut Vec<Violation>) {
    let parts: Vec<Node> = named_children(statement);
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

/// R17 ddl-rest/routes probes, LocalDB 17.0.1000.7, levels 100-170:
/// NULLS on SUM/ROW_NUMBER is Msg 16208; MATCH comma and ungrouped shortest path are Msg 102;
/// shortest-path lower bound != 1 is Msg 13942, upper bound 0 is Msg 13943; ALL DISTINCT is 156.
/// fixture-graph-before/shortest-fixed reject a pipe edge list (102); fixture-graph-after accepts OR of MATCH predicates.
/// Keep legacy raw trees and reject here.
pub(super) fn ddl_rest_routes(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = node.walk();
    let children: Vec<_> = node.syntax_children(&mut cursor).collect();
    let problem = match node.kind() {
        "select" => {
            let all_distinct = children
                .iter()
                .find(|n| n.kind() == "select_expression")
                .and_then(|n| n.syntax_named_child(0))
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
                .and_then(|n| n.syntax_named_child(0))
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
            let previous = batch.syntax_prev_named_sibling();
            let follows_unterminated_batch = previous.is_some_and(|n| {
                named_children(n)
                    .into_iter()
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
pub(super) fn ddl_rest_create(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    match node.kind() {
        "create_index" => rest_create_index(node, source, out),
        "create_aggregate" => {
            if let Some(args) = named_children(node)
                .into_iter()
                .find(|n| n.kind() == "function_arguments")
            {
                let params: Vec<Node> = named_children(args)
                    .into_iter()
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
            if let Some(file) = named_children(node)
                .into_iter()
                .find(|n| n.kind() == "literal")
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
        // batch-rules/certificate-from-*provider*: Msg 102 on LocalDB 17.0.1000.7,
        // levels 100-170 in both sessions. A delimited PROVIDER is an option name.
        "create_certificate"
            if has_child(node, "keyword_provider")
                || direct_options(node).into_iter().any(|option| {
                    matches!(
                        option_name(option, source).as_str(),
                        "PROVIDER" | "[PROVIDER]" | "\"PROVIDER\""
                    )
                }) =>
        {
            out.push(violation(
                "certificate-provider",
                "CREATE CERTIFICATE does not support FROM PROVIDER".into(),
                node,
            ));
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

pub(super) fn rest_create_index(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut c = node.walk();
    let children: Vec<Node> = node.syntax_children(&mut c).collect();
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
            named_children(*cols)
                .into_iter()
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
pub(super) fn ddl_rest_alter(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let kind = statement.kind();
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.syntax_children(&mut cursor).collect();
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
            if kind == "create_login"
                && named_children(option)
                    .into_iter()
                    .any(|n| n.kind() == "identifier" && upper_text(n, source) == "OLD_PASSWORD")
            {
                out.push(violation(
                    "ddl-rest-principal",
                    "OLD_PASSWORD belongs to ALTER, not CREATE LOGIN".into(),
                    option,
                ));
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
pub(super) fn ddl_rest_misc(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let container = node.child_by_field_name("options").or_else(|| {
        named_children(node)
            .into_iter()
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
            if has_child(node, "keyword_query")
                && named_children(node)
                    .into_iter()
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
            for name in named_children(node)
                .into_iter()
                .filter(|n| n.kind() == "object_reference")
            {
                let mut nc = name.walk();
                if !name.syntax_children(&mut nc).any(|n| n.kind() == ".") {
                    reject(
                        out,
                        name,
                        "a sensitivity classification target needs table.column",
                    );
                }
            }
            let mut seen = std::collections::BTreeSet::new();
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
                let duplicate = !seen.insert(name);
                if !valid || duplicate {
                    reject(
                        out,
                        option,
                        "invalid or duplicate sensitivity classification option",
                    );
                }
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
pub(super) fn ddl_rest_closing(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    match node.kind() {
        "parenthesized_expression" => {
            let mut parts = named_children(node).into_iter();
            if !parts.next().is_some_and(|n| n.kind() == "field") || parts.next().is_some() {
                return;
            }
            let mut predicate = false;
            for parent in ancestors(node) {
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
            }
        }
        "drop_sequence" => {
            for name in named_children(node)
                .into_iter()
                .filter(|n| n.kind() == "object_reference")
            {
                let mut cursor = name.walk();
                if name
                    .syntax_children(&mut cursor)
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
            let children: Vec<Node> = node.syntax_children(&mut cursor).collect();
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
                let mut words = named_children(*permission).into_iter();
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
