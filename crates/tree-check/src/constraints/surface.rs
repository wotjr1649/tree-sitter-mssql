use super::Violation;
use super::backup_restore::*;
use super::expr::*;
use super::util::*;
use crate::tree::{has_child, named_children};
use tree_sitter::Node;

/// Surface gaps, LocalDB 17.0.1000.7, levels 100-170 (surface-fix-gaps): a BULK rowset
/// needs an alias (491); without an explicit schema, CSV needs FORMATFILE and the legacy
/// form needs FORMATFILE or SINGLE_* (472/15808). SET assignments cannot feed UNION,
/// EXCEPT or INTERSECT (10734). CREATE SCHEMA owns its batch (111/156).
/// surface-fix-ddl-classification measures crypto WITH (156/319), reserved USER principals (156),
/// scalar CTAS variables (112) and CETAS options (102); platform CTAS/CETAS clauses remain outside.
pub(super) fn surface_shapes(node: Node, source: &[u8], out: &mut Vec<Violation>) {
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
                .syntax_next_named_sibling()
                .filter(|n| n.kind() == "openjson_schema");
            let alias = relation.child_by_field_name("alias").is_some()
                || schema
                    .and_then(|s| s.syntax_next_named_sibling())
                    .is_some_and(|n| {
                        n.kind() == "identifier"
                            || n.kind() == "keyword_as"
                                && n.syntax_next_named_sibling()
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
        "term" => node.child_by_field_name("value").is_some_and(|v| {
            v.kind() == "binary_expression"
                && v.child_by_field_name("operator")
                    .is_some_and(|n| n.kind() == "=")
                && v.child_by_field_name("left")
                    .is_some_and(|n| local_variable(n, source))
                && node
                    .parent()
                    .is_some_and(|p| p.kind() == "select_expression")
                && ancestor(node, "query_specification")
                    .and_then(|q| q.parent())
                    .is_some_and(|p| p.kind() == "set_operation")
        }),
        "alter_cryptographic_provider" => has_child(node, "with_clause"),
        "field" => {
            node.syntax_named_child_count() == 1
                && local_variable(node, source)
                && ancestor(node, "query_specification").is_some()
                && ancestor(node, "create_table").is_some_and(|t| {
                    has_child(t, "keyword_as")
                        && !has_child(t, "keyword_external")
                        && !has_child(t, "column_definitions")
                })
        }
        "create_table" if has_child(node, "keyword_external") && has_child(node, "keyword_as") => {
            named_children(node)
                .into_iter()
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
            let mut previous = node.syntax_prev_named_sibling();
            let mut principal = false;
            while let Some(n) = previous {
                if matches!(n.kind(), "keyword_to" | "keyword_from") {
                    principal = true;
                    break;
                }
                if matches!(n.kind(), "keyword_on" | "keyword_as") {
                    break;
                }
                previous = n.syntax_prev_named_sibling();
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
            let previous = statement.syntax_prev_named_sibling();
            let next = statement
                .syntax_next_named_sibling()
                .filter(|n| n.kind() != "go_statement");
            previous.is_some()
                || next.is_some_and(|next| {
                    let terminated = statement
                        .syntax_next_sibling()
                        .is_some_and(|n| n.kind() == ";");
                    let element = next.syntax_named_child(0);
                    terminated
                        || element.is_none_or(|n| match n.kind() {
                            "create_table" => has_child(n, "keyword_external"),
                            "create_view" | "grant_statement" => false,
                            _ => true,
                        })
                })
        }
        "alter_search_property_list" if has_child(node, "keyword_add") => {
            let options = named_children(node)
                .into_iter()
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
