//! Syntax constraints checked after parsing (ADR-0003); rule families live in `constraints/`.

use crate::tree::preorder_with_depth;
use tree_sitter::Tree;
mod statements;
use statements::*;
mod expr;
use expr::*;
mod module_batch;
use module_batch::*;
mod hints;
use hints::*;
mod external;
use external::*;
mod query;
use query::*;
mod database;
use database::*;
mod backup_restore;
use backup_restore::*;
mod ddl_rest;
use ddl_rest::*;
mod surface;
use surface::*;
mod gap;
use gap::*;
mod util;

#[cfg(test)]
mod tests;

/// A constraint violation: the rule and the row/column (0-based) of the offending node.
#[derive(Debug, PartialEq)]
pub struct Violation {
    pub rule: &'static str,
    pub detail: String,
    pub row: usize,
    pub column: usize,
}

pub fn check(tree: &Tree, source: &[u8]) -> Vec<Violation> {
    // Recovery nodes change with trivia; syntax failure already rejects the input.
    if tree.root_node().has_error() {
        return Vec::new();
    }
    check_syntax(tree, source)
}

fn check_syntax(tree: &Tree, source: &[u8]) -> Vec<Violation> {
    let mut out = Vec::new();
    go_line_inside_token(tree, source, &mut out);
    let mut context = util::Context::default();
    for (node, depth) in preorder_with_depth(tree.root_node()) {
        context.enter(node, depth);
        scalar_predicate(node, &context, source, &mut out);
        surface_shapes(node, &context, source, &mut out);
        match node.kind() {
            "raiserror_statement" | "waitfor_statement" => {
                quoted_string_argument(node, source, &mut out);
            }
            "execute_statement" => {
                quoted_string_argument(node, source, &mut out);
                ddl_rest_routes(node, source, &mut out);
                query_exec(node, &mut out);
                exec_output_requires_at(node, &mut out);
                exec_output_variable(node, source, &mut out);
            }
            "keyword_collate" => {
                collation_name(node, source, &mut out);
            }
            "bulk_insert_statement" => {
                bulk_path(node, source, &mut out);
            }
            "on_filegroup" => {
                filegroup_name(node, source, &mut out);
            }
            "invocation" => {
                gap_invocation(node, &context, source, &mut out);
                builtin_arity(node, source, &mut out);
                expression_language(node, source, &mut out);
                expression_provider(node, source, &mut out);
                json_aggregate(node, &context, source, &mut out);
            }
            "binary_expression" => {
                gap_scalar(node, &context, source, &mut out);
                like_escape(node, source, &mut out);
            }
            "term" | "exec_argument" => {
                gap_scalar(node, &context, source, &mut out);
            }
            "identifier" => {
                gap_name(node, &context, source, &mut out);
            }
            "object_reference" => {
                gap_name(node, &context, source, &mut out);
                static_reference(node, source, &mut out);
            }
            "option" => {
                option_gaps(node, source, &mut out);
                nested_database_options(node, source, &mut out);
            }
            "with_options" | "constraint" => {
                option_gaps(node, source, &mut out);
            }
            "query_hint" | "table_hint" => {
                option_gaps(node, source, &mut out);
                table_hints(node, source, &mut out);
            }
            "block"
            | "try_block"
            | "enable_trigger_statement"
            | "window_frame"
            | "alter_event_session"
            | "create_route"
            | "alter_route"
            | "alter_database"
            | "permission"
            | "trigger_event"
            | "create_server_audit" => {
                gap_statement(node, source, &mut out);
            }
            "create_event_session" | "create_index" => {
                gap_statement(node, source, &mut out);
                ddl_rest_create(node, source, &mut out);
            }
            "alter_server_audit" => {
                gap_statement(node, source, &mut out);
                audit_clause_count(node, &mut out);
            }
            "parenthesized_expression" | "drop_sequence" => {
                ddl_rest_closing(node, source, &mut out);
            }
            "grant_statement" => {
                ddl_rest_closing(node, source, &mut out);
                column_permissions(node, source, &mut out);
                ddl_rest_permissions(node, &mut out);
            }
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
            | "alter_message_type"
            | "alter_authorization" => {
                ddl_rest_alter(node, source, &mut out);
            }
            "alter_queue" => {
                ddl_rest_alter(node, source, &mut out);
                queue_activation(node, source, &mut out);
            }
            "commit_statement"
            | "kill_statement"
            | "update_statistics_statement"
            | "sensitivity_classification_statement"
            | "signature_signer" => {
                ddl_rest_misc(node, source, &mut out);
            }
            "create_queue" => {
                queue_activation(node, source, &mut out);
            }
            "declare_cursor_statement" => {
                query_cursor(node, source, &mut out);
            }
            "set_statement" => {
                query_cursor(node, source, &mut out);
                query_offsets(node, source, &mut out);
            }
            "atomic_clause" => {
                query_atomic(node, source, &mut out);
            }
            "select" => {
                query_select(node, &mut out);
                ddl_rest_routes(node, source, &mut out);
            }
            "function_argument" => {
                query_parameter(node, &mut out);
            }
            "cast" => {
                query_cast(node, &mut out);
            }
            "openjson_schema" | "openxml_table_schema" => {
                query_rowset_schema(node, source, &mut out);
            }
            "delete" | "update" | "insert" => {
                query_dml_source(node, &context, &mut out);
            }
            "assignment" => {
                query_assignment(node, source, &mut out);
            }
            "create_procedure" | "alter_procedure" => {
                query_module(node, &mut out);
                batch_module(node, &mut out);
            }
            "create_function" | "alter_function" => {
                query_module(node, &mut out);
                scalar_function_return(node, &mut out);
                batch_module(node, &mut out);
            }
            "create_trigger" | "alter_trigger" => {
                query_module(node, &mut out);
                trigger_scope(node, source, &mut out);
                batch_module(node, &mut out);
            }
            "create_endpoint" | "alter_endpoint" => {
                endpoint_options(node, source, &mut out);
            }
            "create_fulltext_catalog" | "create_fulltext_index" => {
                fulltext_options(node, source, &mut out);
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
            | "alter_external_resource_pool" => {
                external_options(node, source, &mut out);
            }
            "statement" => {
                cte_terminator(node, &mut out);
                cte_table_source(node, &mut out);
            }
            "query_specification" => {
                select_into_scope(node, &mut out);
            }
            "set_operation" => {
                nested_set_clauses(node, &context, &mut out);
            }
            "field" => {
                static_reference(node, source, &mut out);
            }
            "window_specification" => {
                window_frame_order(node, &mut out);
            }
            "window_function" | "graph_match_expr" | "graph_shortest_path" | "graph_hop" => {
                ddl_rest_routes(node, source, &mut out);
            }
            "merge" => {
                merge_terminator(node, &mut out);
            }
            "column_definitions" => {
                table_variable_foreign_key(node, &mut out);
            }
            "option_clause" => {
                table_hint_aliases(node, source, &mut out);
            }
            "database_file" => {
                database_file_options(node, source, &mut out);
            }
            "availability_replica" => {
                availability_replica_options(node, source, &mut out);
            }
            "server_buffer_pool_options" => {
                buffer_pool_options(node, source, &mut out);
            }
            "alter_availability_group" => {
                availability_set_options(node, &mut out);
            }
            "create_database_encryption_key" | "alter_database_encryption_key" => {
                encryption_algorithm(node, source, &mut out);
            }
            "backup_statement" | "restore_statement" => {
                backup_restore(node, source, &mut out);
            }
            "create_aggregate" | "create_cryptographic_provider" | "create_asymmetric_key" => {
                ddl_rest_create(node, source, &mut out);
            }
            "create_view" | "alter_view" | "create_rule" | "create_default" => {
                batch_module(node, &mut out);
            }
            _ => {}
        }
    }
    out
}
