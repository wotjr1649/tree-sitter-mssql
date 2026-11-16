import { comma_list, paren_list, wrapped_in_parenthesis } from '../helpers.js';

import create_function_rules from './create-function.js';
import create_procedure_rules from './create-procedure.js';

// The ADD EVENT / ADD TARGET / DROP EVENT / DROP TARGET / WITH clause body
// shared by CREATE EVENT SESSION and the non-STATE form of ALTER EVENT
// SESSION (alter_event_session in alter.js — STATE = value must be that
// statement's only clause, so it is not part of this body). Nonempty
// suffix rules share the optional clause combinations between both statements.
// Fixes the clause order to ADD EVENT*, ADD TARGET*, DROP EVENT*, DROP
// TARGET*, WITH — SQL Server itself allows these in any order and
// interleaved, but every generated script (SSMS, DDL-triggers) emits them
// this way, and a stricter, order-sensitive grammar is what lets this rule
// stay conflict-free without a `conflicts` entry. A script that reorders or
// interleaves these clauses hits an ERROR node; widen the order here if
// that turns up in real use.
export function event_session_body($) {
  return optional($._event_session_body);
}

// The `TO { FILE (options) | APPLICATION_LOG | SECURITY_LOG }` audit-target
// clause shared, character for character, by CREATE SERVER AUDIT (below,
// as a fixed sequence) and ALTER SERVER AUDIT (as one alternative of its
// own repeated, any-order clause choice).
export function server_audit_target_clause($) {
  return seq(
    $.keyword_to,
    choice(seq(alias($.keyword_file, $.identifier), paren_list($.option, true)), $.identifier),
  );
}

export default {
  _event_session_body: ($) =>
    prec.right(
      choice(
        seq(comma_list($._add_event_clause, true), optional($._event_session_after_events)),
        $._event_session_after_events,
      ),
    ),
  _event_session_after_events: ($) =>
    prec.right(
      choice(
        seq(comma_list($._add_target_clause, true), optional($._event_session_after_targets)),
        $._event_session_after_targets,
      ),
    ),
  _event_session_after_targets: ($) =>
    prec.right(
      choice(
        seq(comma_list($._drop_event_clause, true), optional($._event_session_after_drop_events)),
        $._event_session_after_drop_events,
      ),
    ),
  _event_session_after_drop_events: ($) =>
    prec.right(
      choice(
        seq(comma_list($._drop_target_clause, true), optional($.with_options)),
        $.with_options,
      ),
    ),

  _create_statement: ($) =>
    choice(
      $.create_table,
      $.create_external_model,
      $.create_external_language,
      $.create_external_library,
      $.create_external_resource_pool,
      $.create_endpoint,
      $.create_fulltext_stoplist,
      $.create_search_property_list,
      $.create_view,
      $.create_index,
      $.create_function,
      $.create_procedure,
      $.create_trigger,
      $.create_type,
      $.create_schema,
      $.create_database,
      $.create_database_encryption_key,
      $.create_database_audit_specification,
      $.create_server_audit_specification,
      $.create_availability_group,
      $.create_role,
      $.create_user,
      $.create_login,
      $.create_synonym,
      $.create_sequence,
      $.create_fulltext_catalog,
      $.create_fulltext_index,
      $.create_route,
      $.create_server_audit,
      $.create_assembly,
      $.create_queue,
      $.create_contract,
      $.create_message_type,
      $.create_partition_function,
      $.create_partition_scheme,
      $.create_external_data_source,
      $.create_external_file_format,
      $.create_statistics,
      $.create_event_session,
      $.create_master_key,
      $.create_certificate,
      $.create_symmetric_key,
      $.create_asymmetric_key,
      $.create_column_encryption_key,
      $.create_column_master_key,
      $.create_security_policy,
      $.create_xml_schema_collection,
      $.create_aggregate,
      $.create_application_role,
      $.create_credential,
      $.create_cryptographic_provider,
      $.create_default,
      $.create_rule,
      $.create_broker_priority,
      $.create_service,
      $.create_remote_service_binding,
      $.create_resource_pool,
      $.create_workload_group,
      $.create_event_notification,
    ),

  // CREATE XML SCHEMA COLLECTION name AS expression
  create_xml_schema_collection: ($) =>
    seq(
      $.keyword_create,
      $.keyword_xml,
      $.keyword_schema,
      $.keyword_collection,
      field('name', $.object_reference),
      $.keyword_as,
      $._expression,
    ),

  // CREATE [EXTERNAL] TABLE name ( column | constraint [, ...] )
  //   [ON filegroup] [TEXTIMAGE_ON filegroup] [FILESTREAM_ON filegroup] [WITH (options)]
  // An external table's required WITH (DATA_SOURCE = ..., ...) is the same
  // option list. The four trailing clauses are each optional and appear at
  // most once, in this fixed order, per SQL Server's own syntax reference —
  // not an unbounded repeat of arbitrary clause kinds. TEXTIMAGE_ON and
  // FILESTREAM_ON are independent (a table with both a large-value column
  // and a FILESTREAM column needs both), so each gets its own slot rather
  // than sharing one `table_data_placement` between them.
  create_table: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        optional($.keyword_external),
        $.keyword_table,
        field('name', $.object_reference),
        choice(
          seq($.with_options, $.keyword_as, $._dml_read),
          seq(
            choice(
              seq(
                alias($._create_table_columns, $.column_definitions),
                optional(alias($._graph_table_kind, $.table_data_placement)),
              ),
              seq($.keyword_as, choice($.keyword_filetable, $.keyword_edge)),
            ),
            optional($.on_filegroup),
            optional($.table_data_placement),
            optional($.table_data_placement),
            optional($.with_options),
          ),
        ),
      ),
    ),

  _create_table_columns: ($) =>
    wrapped_in_parenthesis(
      seq(
        comma_list(choice($.column_definition, $.constraint, $.period_for_system_time), true),
        optional(','),
      ),
    ),

  // Keep the existing AS NODE/EDGE tree, originally recognized as data placement.
  _graph_table_kind: ($) =>
    seq(
      field('option', alias($.keyword_as, $.identifier)),
      field(
        'filegroup',
        choice(alias($.keyword_node, $.identifier), alias($.keyword_edge, $.identifier)),
      ),
    ),

  // TEXTIMAGE_ON filegroup | FILESTREAM_ON filegroup — where the filegroup
  // name can also be the literal keyword DEFAULT, hence a bare identifier
  // rather than an enumerated set.
  table_data_placement: ($) =>
    seq(field('option', $.identifier), field('filegroup', $._filegroup_name)),

  // CREATE [OR ALTER] VIEW name [(columns)] [WITH option [, ...]] AS select [WITH CHECK OPTION]
  create_view: ($) => seq($.keyword_create, optional($._or_alter), $._view_definition),

  alter_view: ($) => seq($.keyword_alter, $._view_definition),

  _view_definition: ($) =>
    prec.right(
      seq(
        $.keyword_view,
        field('name', $.object_reference),
        optional(paren_list($.identifier, true)),
        optional(seq($.keyword_with, comma_list($.view_option, true))),
        $.keyword_as,
        $._dml_read,
        optional(seq($.keyword_with, $._check_option)),
      ),
    ),

  view_option: ($) =>
    choice(
      $.keyword_encryption,
      $.keyword_schemabinding,
      // VIEW_METADATA
      $.identifier,
    ),

  // CREATE [UNIQUE] [CLUSTERED | NONCLUSTERED] [COLUMNSTORE] INDEX name ON table [(column [ASC | DESC] [, ...])]
  //   [ORDER (columns)] [INCLUDE (columns)] [WHERE predicate] [WITH (options)] [ON filegroup]
  // ORDER (...) is the ordered clustered columnstore form (2022). COLUMNSTORE
  // is typed since it's the common case; a bare identifier fallback keeps
  // other single-word storage modifiers (e.g. SPATIAL) parsing the way they
  // did before this slot was typed.
  create_index: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        optional($.keyword_unique),
        optional(choice($.keyword_primary, $.keyword_selective)),
        optional($._index_kind),
        optional(
          field(
            'storage',
            choice($.keyword_columnstore, alias($.keyword_xml, $.identifier), $.identifier),
          ),
        ),
        $.keyword_index,
        field('name', $.identifier),
        $.keyword_on,
        $.object_reference,
        optional($.ordered_columns),
        optional($._create_index_suffix),
      ),
    ),

  // A nonempty suffix shares trailing states without inheriting header fields.
  _create_index_suffix: ($) => {
    const clauses = [
      seq($.keyword_order, paren_list($.identifier, true)),
      $.covering_columns,
      $.where,
      $._index_using_or_paths,
      choice(
        alias($._create_index_with_options, $.with_options),
        alias($._legacy_index_with_clause, $.with_clause),
      ),
      $.on_filegroup,
      alias($._index_filestream_placement, $.table_data_placement),
    ];
    return prec.right(
      choice(...clauses.map((clause, i) => seq(clause, ...clauses.slice(i + 1).map(optional)))),
    );
  },

  _legacy_index_with_clause: ($) =>
    seq($.keyword_with, comma_list(alias($._legacy_index_option, $.option), true)),

  // Bare legacy switches have no whitespace-separated value; ON starts data placement.
  _legacy_index_option: ($) =>
    seq(field('name', $.identifier), optional(seq('=', field('value', $._option_value)))),

  _index_using_or_paths: ($) =>
    choice(
      seq(
        $.keyword_using,
        choice(
          $.identifier,
          seq(
            $.keyword_xml,
            $.keyword_index,
            $.identifier,
            optional(
              seq(
                $.keyword_for,
                choice(
                  $.identifier,
                  $.keyword_value,
                  $.keyword_property,
                  paren_list($.identifier, true),
                ),
              ),
            ),
          ),
        ),
      ),
      seq(
        optional(seq($.keyword_with, $.xmlnamespaces)),
        $.keyword_for,
        paren_list(choice($.selective_xml_path, $.literal), true),
      ),
    ),

  _create_index_with_options: ($) =>
    seq(
      $.keyword_with,
      paren_list(
        choice(
          $.option,
          alias($._compression_partitions, $.option),
          alias($._move_to_option, $.option),
          alias($._index_tuple_option, $.option),
        ),
        true,
      ),
    ),

  _index_tuple_option: ($) =>
    seq(
      field('name', $.identifier),
      '=',
      paren_list(choice($.literal, $.option, $.unary_expression), true),
    ),

  _index_filestream_placement: ($) =>
    seq(
      field('option', alias($.keyword_filestream_on, $.identifier)),
      field('filegroup', $._filegroup_name),
    ),

  covering_columns: ($) => seq($.keyword_include, paren_list($.identifier, true)),

  ...create_function_rules,
  ...create_procedure_rules,

  // T-SQL trigger:
  //
  //   CREATE [OR ALTER] TRIGGER name ON { table | DATABASE | ALL SERVER }
  //       [ WITH { ENCRYPTION | EXECUTE AS ... } [, ...] ]
  //       { FOR | AFTER | INSTEAD OF } event [, ...]
  //       [ WITH APPEND ] [ NOT FOR REPLICATION ]
  //   AS body
  //
  // DML events are INSERT/UPDATE/DELETE; DDL events (CREATE_TABLE,
  // DDL_DATABASE_LEVEL_EVENTS) and LOGON are identifiers.
  create_trigger: ($) => seq($.keyword_create, optional($._or_alter), $._trigger_definition),

  alter_trigger: ($) => seq($.keyword_alter, $._trigger_definition),

  _trigger_definition: ($) =>
    seq(
      $.keyword_trigger,
      field('name', $.object_reference),
      $.keyword_on,
      choice(
        seq($.keyword_all, $.keyword_server),
        $.keyword_database,
        field('target', $.object_reference),
      ),
      optional($.trigger_options),
      choice($.keyword_for, $.keyword_after, seq($.keyword_instead, $.keyword_of)),
      comma_list($.trigger_event, true),
      optional(seq($.keyword_with, $.keyword_append)),
      optional($._not_for_replication),
      field('body', $.procedure_body),
    ),

  trigger_options: ($) => seq($.keyword_with, comma_list($.trigger_option, true)),

  trigger_option: ($) =>
    choice(
      $.keyword_encryption,
      $.keyword_native_compilation,
      $.keyword_schemabinding,
      $.execute_as_clause,
    ),

  trigger_event: ($) => choice($.keyword_insert, $.keyword_update, $.keyword_delete, $.identifier),

  // CREATE TYPE name FROM base_type [NULL | NOT NULL]        (alias type)
  // CREATE TYPE name AS TABLE ( ... ) [WITH (options)]       (table type)
  create_type: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_type,
        field('name', $.object_reference),
        choice(
          seq($.keyword_from, $._type, optional(choice($.keyword_null, $._not_null))),
          seq($.keyword_as, $.keyword_table, $.column_definitions, optional($.with_options)),
          $.external_name,
        ),
      ),
    ),

  // CREATE SCHEMA name [AUTHORIZATION owner] | CREATE SCHEMA AUTHORIZATION owner
  create_schema: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_schema,
        choice(
          seq(field('name', $.identifier), optional(seq($.keyword_authorization, $.identifier))),
          seq($.keyword_authorization, $.identifier),
        ),
      ),
    ),

  // File specifications are shared with ALTER DATABASE; table/index options stay separate.
  create_database: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_database,
        field('name', $._database_name),
        optional(seq($.keyword_containment, '=', choice($.keyword_none, $.keyword_partial))),
        optional($._database_files),
        optional(seq($.keyword_log, $.keyword_on, comma_list($.database_file, true))),
        optional(seq($.keyword_collate, $.identifier)),
        optional(
          choice(
            seq(
              $.keyword_for,
              choice($.keyword_attach, $.keyword_attach_rebuild_log),
              optional(seq($.keyword_with, comma_list(alias($.database_option, $.option), true))),
            ),
            seq($.keyword_as, $.keyword_snapshot, $.keyword_of, $.identifier),
            seq($.keyword_with, comma_list(alias($.database_option, $.option), true)),
          ),
        ),
      ),
    ),

  // CREATE [SERVER] ROLE name [AUTHORIZATION owner]
  create_role: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        optional($.keyword_server),
        $.keyword_role,
        field('name', $.identifier),
        optional(seq($.keyword_authorization, $.identifier)),
      ),
    ),

  // CREATE USER name [ { FOR | FROM } LOGIN login | WITHOUT LOGIN ] [WITH option [, ...]]
  create_user: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_user,
        field('name', $.identifier),
        optional(
          choice(
            seq(
              choice($.keyword_for, $.keyword_from),
              choice(
                seq(choice($.keyword_login, $.keyword_certificate), $.identifier),
                seq($.keyword_asymmetric, $.keyword_key, $.identifier),
                seq($.keyword_external, $.keyword_provider),
              ),
            ),
            seq($.keyword_without, $.keyword_login),
          ),
        ),
        optional($.with_clause),
      ),
    ),

  // CREATE LOGIN name { FROM WINDOWS | FROM CERTIFICATE name | FROM ASYMMETRIC KEY name | FROM EXTERNAL PROVIDER } [WITH option [, ...]]
  // CREATE LOGIN name WITH PASSWORD = 'password' [, option ...]
  // FROM EXTERNAL PROVIDER is the Microsoft Entra login form on Azure SQL
  // Database, Azure SQL Managed Instance and SQL Server 2022+.
  create_login: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_login,
        field('name', $.identifier),
        optional(
          seq(
            $.keyword_from,
            choice(
              $.keyword_windows,
              seq($.keyword_certificate, field('source', $.identifier)),
              seq($.keyword_asymmetric, $.keyword_key, field('source', $.identifier)),
              seq($.keyword_external, $.keyword_provider),
            ),
          ),
        ),
        optional(alias($._login_with_clause, $.with_clause)),
      ),
    ),

  // CREATE SYNONYM name FOR object
  create_synonym: ($) =>
    seq(
      $.keyword_create,
      $.keyword_synonym,
      field('name', $.object_reference),
      $.keyword_for,
      $.object_reference,
    ),

  // CREATE SEQUENCE name [AS type] [START WITH n] [INCREMENT BY n] [{MINVALUE n | NO MINVALUE}]
  //   [{MAXVALUE n | NO MAXVALUE}] [{CYCLE | NO CYCLE}] [{CACHE [n] | NO CACHE}]
  create_sequence: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_sequence,
        field('name', $.object_reference),
        repeat($.sequence_option),
      ),
    ),

  sequence_option: ($) =>
    prec.right(
      choice(
        seq($.keyword_as, $._type),
        seq($.keyword_start, $.keyword_with, $._expression),
        seq($.keyword_restart, optional(seq($.keyword_with, $._expression))),
        seq($.keyword_increment, $.keyword_by, $._expression),
        seq($.keyword_minvalue, $._expression),
        seq($.keyword_maxvalue, $._expression),
        seq(
          $.keyword_no,
          choice($.keyword_minvalue, $.keyword_maxvalue, $.keyword_cycle, $.keyword_cache),
        ),
        $.keyword_cycle,
        seq($.keyword_cache, optional($._expression)),
      ),
    ),

  // CREATE FULLTEXT CATALOG name [ON FILEGROUP fg] [IN PATH 'path'] [WITH option] [AS DEFAULT] [AUTHORIZATION owner]
  create_fulltext_catalog: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_fulltext,
        $.keyword_catalog,
        $.identifier,
        repeat(
          choice(
            seq($.keyword_on, $.identifier, $.identifier),
            seq($.keyword_in, $.identifier, $.literal),
            $.with_clause,
            seq($.keyword_as, $.keyword_default),
            seq($.keyword_authorization, $.identifier),
          ),
        ),
      ),
    ),

  // CREATE ROUTE name [AUTHORIZATION owner] WITH option [, ...]   (Service Broker)
  create_route: ($) =>
    seq(
      $.keyword_create,
      $.keyword_route,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      $.with_clause,
    ),

  // CREATE SERVER AUDIT name TO { FILE (options) | APPLICATION_LOG | SECURITY_LOG } [WITH (options)] [WHERE predicate]
  create_server_audit: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_server,
        $.keyword_audit,
        field('name', $.identifier),
        server_audit_target_clause($),
        optional($.with_options),
        optional($.where),
      ),
    ),

  // ALTER SERVER AUDIT name [TO ...] [WITH (options)] [WHERE predicate] [MODIFY NAME = name]
  alter_server_audit: ($) =>
    prec.right(
      seq(
        $.keyword_alter,
        $.keyword_server,
        $.keyword_audit,
        field('name', $.identifier),
        choice(
          seq($.keyword_remove, $.keyword_where),
          repeat1(
            choice(
              server_audit_target_clause($),
              $.with_options,
              $.where,
              seq($.keyword_modify, $.option),
            ),
          ),
        ),
      ),
    ),

  // CREATE ASSEMBLY name [AUTHORIZATION owner] FROM { 'path' | 0x... } [, ...] [WITH PERMISSION_SET = ...]
  create_assembly: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_assembly,
        field('name', $.identifier),
        optional(seq($.keyword_authorization, $.identifier)),
        $.keyword_from,
        comma_list($._assembly_source, true),
        optional($.with_clause),
      ),
    ),

  _assembly_source: ($) =>
    prec(2, choice($.literal, alias($._tsql_parameter, $.identifier), $.binary_expression)),

  // CREATE FULLTEXT INDEX ON table (column [TYPE COLUMN col] [LANGUAGE lang] [STATISTICAL_SEMANTICS] [, ...])
  //   KEY INDEX index_name [ON catalog] [WITH options]
  // TYPE COLUMN / LANGUAGE / STATISTICAL_SEMANTICS are option names, not keywords — same rule as
  // every other DDL option list: a word that is a keyword elsewhere lexes as an identifier here
  // because keyword extraction only substitutes a keyword where it is grammatically valid.
  create_fulltext_index: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_fulltext,
        $.keyword_index,
        $.keyword_on,
        $.object_reference,
        optional(paren_list($.fulltext_index_column, true)),
        $.keyword_key,
        $.keyword_index,
        $.identifier,
        optional(seq($.keyword_on, $._fulltext_catalog_filegroup)),
        optional(
          choice(
            alias($._fulltext_with_clause, $.with_clause),
            alias($._fulltext_with_options, $.with_options),
          ),
        ),
      ),
    ),

  fulltext_index_column: ($) =>
    seq(field('name', $.identifier), repeat(choice($.identifier, $.literal))),

  // CREATE QUEUE name [WITH options] [ON filegroup]   (Service Broker)
  // Each clause is optional and appears at most once, WITH then ON. An
  // unbounded repeat of the two accepted both any number of times and in
  // either order. No prec.right: when ON is omitted the WITH list is the
  // trailing clause, and forcing that shift swallows a following CTE. The
  // [$.create_queue] conflicts entry forks that case instead.
  create_queue: ($) =>
    seq(
      $.keyword_create,
      $.keyword_queue,
      field('name', $.object_reference),
      optional(alias($._create_queue_with_clause, $.with_clause)),
      optional($.on_filegroup),
    ),

  // CREATE CONTRACT name ( {DEFAULT | message_type} SENT BY {INITIATOR|TARGET|ANY} [, ...] )   (Service Broker)
  create_contract: ($) =>
    seq(
      $.keyword_create,
      $.keyword_contract,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      paren_list($.contract_message, true),
    ),

  contract_message: ($) =>
    seq(
      choice($.keyword_default, $.identifier),
      $.keyword_sent,
      $.keyword_by,
      choice($.keyword_initiator, $.keyword_target, $.keyword_any),
    ),

  // CREATE MESSAGE TYPE name [AUTHORIZATION owner] [VALIDATION = value ...]   (Service Broker)
  // No ALTER MESSAGE TYPE exists in real SQL Server — a message type is
  // immutable once created, unlike QUEUE/ASSEMBLY/FULLTEXT CATALOG/
  // FULLTEXT INDEX/PARTITION FUNCTION/PARTITION SCHEME/ROUTE, which all
  // have a real ALTER form in alter.js.
  // VALIDATION's values (NONE, EMPTY, WELL_FORMED_XML, VALID_XML [WITH SCHEMA COLLECTION name])
  // are identifiers for the same reason DDL option values are throughout this grammar. VALID_XML
  // can itself be followed by WITH SCHEMA COLLECTION naming an XML schema collection, which needs
  // an explicit alternative since keyword_with/keyword_schema/keyword_collection aren't identifiers
  // and can't be swallowed by a bare identifier run.
  // No prec.right here: the last clause's trailing WITH suffix creates the
  // same shift/reduce ambiguity as RECONFIGURE/RAISERROR's trailing WITH
  // (a following unterminated statement's CTE), and forcing the shift with
  // prec.right breaks that case; the [$.create_message_type] conflicts
  // entry lets GLR resolve both that and the bare-identifier VALIDATION
  // binding correctly instead.
  create_message_type: ($) =>
    seq(
      $.keyword_create,
      $.keyword_message,
      $.keyword_type,
      $.identifier,
      optional(seq($.keyword_authorization, $.identifier)),
      optional(
        seq(
          $.identifier,
          '=',
          $.identifier,
          optional(
            seq(
              $.keyword_with,
              $.keyword_schema,
              $.keyword_collection,
              field('name', $.object_reference),
            ),
          ),
        ),
      ),
    ),

  // CREATE PARTITION FUNCTION name (type) AS RANGE [LEFT|RIGHT] FOR VALUES (boundary [, ...])
  // Boundary values are expressions, not literals. Sliding-window maintenance
  // computes the next boundary into a variable and passes that here, the
  // same way ALTER PARTITION FUNCTION's SPLIT/MERGE boundary already accepts
  // an expression.
  create_partition_function: ($) =>
    seq(
      $.keyword_create,
      $.keyword_partition,
      $.keyword_function,
      field('name', $.identifier),
      wrapped_in_parenthesis($._type),
      $.keyword_as,
      $.keyword_range,
      optional(choice($.keyword_left, $.keyword_right)),
      $.keyword_for,
      $.keyword_values,
      paren_list($._expression, false),
    ),

  // CREATE EXTERNAL DATA SOURCE name WITH (TYPE = ..., LOCATION = '...', ...)
  create_external_data_source: ($) =>
    seq(
      $.keyword_create,
      $.keyword_external,
      $.keyword_data,
      $.keyword_source,
      field('name', $.identifier),
      $.with_options,
    ),

  // CREATE EXTERNAL FILE FORMAT name WITH (FORMAT_TYPE = ..., FORMAT_OPTIONS (...))
  create_external_file_format: ($) =>
    seq(
      $.keyword_create,
      $.keyword_external,
      $.keyword_file,
      $.keyword_format,
      field('name', $.identifier),
      $.with_options,
    ),

  // CREATE STATISTICS name ON table (column [, ...]) [WHERE predicate] [WITH option [, ...]]
  // WITH SAMPLE 5 PERCENT, FULLSCAN, NORECOMPUTE are the bare option list.
  create_statistics: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_statistics,
        field('name', $.identifier),
        $.keyword_on,
        $.object_reference,
        paren_list($.identifier, true),
        optional($.where),
        optional($.with_clause),
      ),
    ),

  // CREATE PARTITION SCHEME name AS PARTITION function_name [ALL] TO (filegroup [, ...])
  create_partition_scheme: ($) =>
    seq(
      $.keyword_create,
      $.keyword_partition,
      $.keyword_scheme,
      field('name', $.identifier),
      $.keyword_as,
      $.keyword_partition,
      $.identifier,
      optional($.keyword_all),
      $.keyword_to,
      paren_list(choice($.identifier, alias($._literal_string, $.literal)), true),
    ),

  // CREATE EVENT SESSION name ON SERVER
  //   [ADD EVENT event(...) [, ...]] [ADD TARGET target(...) [, ...]] [WITH (options)]
  // Shares its clause body with ALTER EVENT SESSION's ADD/DROP/WITH form
  // via the event_session_body() helper below.
  create_event_session: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_event,
        $.keyword_session,
        field('name', $.identifier),
        $.keyword_on,
        choice($.keyword_server, $.keyword_database),
        event_session_body($),
      ),
    ),

  _add_event_clause: ($) => seq($.keyword_add, $.keyword_event, $.event_session_event),
  _add_target_clause: ($) => seq($.keyword_add, $.keyword_target, $.event_session_target),
  // Event and target names in DROP EVENT / DROP TARGET are dotted the same
  // way an ADD EVENT / ADD TARGET name is (`sqlserver.lock_deadlock`), and
  // field-tagged `name` the same way event_session_event/event_session_target
  // tag theirs, so a consumer can find the name the same way regardless of
  // which clause introduced or removed it.
  _drop_event_clause: ($) =>
    seq($.keyword_drop, $.keyword_event, field('name', $.object_reference)),
  _drop_target_clause: ($) =>
    seq($.keyword_drop, $.keyword_target, field('name', $.object_reference)),

  // event_module_guid [( [SET attr=value [, ...]] [ACTION (action [, ...])] [WHERE predicate] )]
  // Event and action names are dotted (`sqlserver.sql_statement_completed`),
  // so both are object_references rather than bare identifiers.
  event_session_event: ($) =>
    prec.right(
      seq(
        field('name', $.object_reference),
        optional(
          wrapped_in_parenthesis(
            repeat(
              choice(
                seq($.keyword_set, comma_list($.option, true)),
                seq($.keyword_action, paren_list($.object_reference, true)),
                $.where,
              ),
            ),
          ),
        ),
      ),
    ),

  // target_module_guid [(SET param=value [, ...])]
  event_session_target: ($) =>
    prec.right(
      seq(
        field('name', $.object_reference),
        optional(wrapped_in_parenthesis(seq($.keyword_set, comma_list($.option, true)))),
      ),
    ),
  create_aggregate: ($) =>
    seq(
      $.keyword_create,
      $.keyword_aggregate,
      field('name', $.object_reference),
      $.function_arguments,
      $.keyword_returns,
      $._type,
      $.external_name,
    ),

  create_application_role: ($) =>
    seq(
      $.keyword_create,
      $.keyword_application,
      $.keyword_role,
      field('name', $.identifier),
      $.with_clause,
    ),

  create_credential: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        optional(seq($.keyword_database, $.keyword_scoped)),
        $.keyword_credential,
        field('name', $.identifier),
        $.with_clause,
        optional(seq($.keyword_for, $.keyword_cryptographic, $.keyword_provider, $.identifier)),
      ),
    ),

  create_default: ($) =>
    seq(
      $.keyword_create,
      $.keyword_default,
      field('name', $.object_reference),
      $.keyword_as,
      $._expression,
    ),

  create_rule: ($) =>
    seq(
      $.keyword_create,
      $.keyword_rule,
      field('name', $.object_reference),
      $.keyword_as,
      $._expression,
    ),

  create_broker_priority: ($) => seq($.keyword_create, $._broker_priority_body),

  _broker_priority_body: ($) =>
    seq(
      $.keyword_broker,
      $.keyword_priority,
      field('name', $.identifier),
      $.keyword_for,
      $.keyword_conversation,
      $.keyword_set,
      paren_list($.option, true),
    ),

  create_service: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_service,
        field('name', $.identifier),
        optional(seq($.keyword_authorization, $.identifier)),
        $.keyword_on,
        $.keyword_queue,
        $.object_reference,
        optional(paren_list($.identifier, true)),
      ),
    ),

  create_remote_service_binding: ($) =>
    seq(
      $.keyword_create,
      $.keyword_remote,
      $.keyword_service,
      $.keyword_binding,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      $.keyword_to,
      $.keyword_service,
      $.literal,
      alias($._remote_binding_with_clause, $.with_clause),
    ),

  create_resource_pool: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_resource,
        $.keyword_pool,
        field('name', $.identifier),
        optional(alias($._resource_pool_options, $.with_options)),
      ),
    ),

  _resource_pool_options: ($) =>
    seq(
      $.keyword_with,
      paren_list(choice($.option, alias($._resource_pool_affinity, $.option)), true),
    ),

  _resource_pool_affinity: ($) =>
    seq(
      field('name', alias($.keyword_affinity, $.identifier)),
      choice($.keyword_scheduler, $.keyword_numanode),
      '=',
      choice($.identifier, paren_list($.external_cpu_range, true)),
    ),

  create_workload_group: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_workload,
        $.keyword_group,
        field('name', $.identifier),
        optional($._workload_group_tail),
      ),
    ),

  _workload_group_tail: ($) =>
    choice(seq($.with_options, optional($._workload_group_using)), $._workload_group_using),

  _workload_group_using: ($) =>
    seq(
      $.keyword_using,
      choice(
        seq($.identifier, optional(seq(',', $.keyword_external, $.identifier))),
        seq($.keyword_external, $.identifier),
      ),
    ),

  create_event_notification: ($) =>
    seq(
      $.keyword_create,
      $.keyword_event,
      $.keyword_notification,
      field('name', $.identifier),
      $._event_notification_scope,
      optional(seq($.keyword_with, $.keyword_fan_in)),
      $.keyword_for,
      comma_list($.identifier, true),
      $.keyword_to,
      $.keyword_service,
      $.literal,
      ',',
      $.literal,
    ),

  _event_notification_scope: ($) =>
    seq(
      $.keyword_on,
      choice($.keyword_server, $.keyword_database, seq($.keyword_queue, $.object_reference)),
    ),
};
