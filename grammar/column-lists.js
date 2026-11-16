import { comma_list, paren_list, wrapped_in_parenthesis } from './helpers.js';

// The tail shared by every index-shaped constraint form: PRIMARY KEY,
// UNIQUE, and an inline INDEX, at both column level (no explicit column
// list — the constraint's own column is implied) and table level (an
// explicit `ordered_columns` list).
function index_spec($, { columns, optionalColumns } = {}) {
  return [
    optional($._constraint_index_kind),
    ...(columns ? [optionalColumns ? optional($.ordered_columns) : $.ordered_columns] : []),
    optional($._constraint_index_options),
    optional($.on_filegroup),
  ];
}

// Column definitions and constraints, shared by CREATE TABLE, ALTER TABLE
// ... ADD, DECLARE @t TABLE, CREATE TYPE ... AS TABLE, and multi-statement
// table-valued function return tables.
export default {
  _column_list: ($) => paren_list(alias($._column, $.column), true),

  _column: ($) =>
    choice(
      $.identifier,
      alias($.keyword_period, $.identifier),
      alias($.keyword_connection, $.identifier),
    ),

  // ( column | constraint [, ...] ) — T-SQL allows a table-level constraint
  // anywhere in the list, between columns as well as at the end.
  //
  // `column_definitions` is shared by CREATE TABLE, ALTER TABLE ... ADD,
  // CREATE TYPE ... AS TABLE, and every table variable declaration
  // (DECLARE @t TABLE, a multi-statement TVF's RETURNS @t TABLE), so
  // `period_for_system_time` is syntactically reachable in the latter two
  // even though only a durable base table can be system-versioned.
  // Deliberate, per the same over-acceptance reasoning `relation`'s own
  // comment states above.
  column_definitions: ($) =>
    paren_list(choice($.column_definition, $.constraint, $.period_for_system_time), true),

  // Keep the atomic token's existing tree; the split form also allows comments.
  period_for_system_time: ($) =>
    seq($._period_phrase, wrapped_in_parenthesis(seq($.identifier, ',', $.identifier))),

  _period_phrase: ($) =>
    choice(
      $.keyword_period_for_system_time,
      alias($._separated_period_phrase, $.keyword_period_for_system_time),
    ),
  _separated_period_phrase: ($) => seq($.keyword_period, $.keyword_for, $.keyword_system_time),

  // name type [COLLATE name] [constraint ...]
  // name AS expression [PERSISTED] [NOT NULL]       (computed column)
  column_definition: ($) =>
    prec.left(
      choice(
        field('type', $.keyword_timestamp),
        seq(
          field('name', choice($._column, alias($.keyword_timestamp, $.identifier))),
          choice(
            seq(
              field('type', $._type),
              optional(seq($.keyword_collate, $.identifier)),
              repeat($._column_constraint),
            ),
            seq(
              $.keyword_as,
              field('expression', $._expression),
              optional($.keyword_persisted),
              optional($._not_null),
              repeat($._column_key_constraint),
            ),
          ),
        ),
      ),
    ),

  // IDENTITY can carry NOT FOR REPLICATION; computed columns share the reference suffix.
  _column_constraint: ($) =>
    prec.right(
      choice(
        $.keyword_null,
        $._not_null,
        $._not_for_replication,
        // IDENTITY [(seed, increment)]
        seq(
          $.keyword_identity,
          optional(
            wrapped_in_parenthesis(
              seq(field('seed', $._expression), ',', field('increment', $._expression)),
            ),
          ),
        ),
        // End a DEFAULT scalar before NOT NULL, while retaining arithmetic and WITH VALUES.
        prec.right(
          'default_value',
          seq(
            optional(seq($.keyword_constraint, field('name', $.identifier))),
            $.keyword_default,
            $._expression,
            optional(seq($.keyword_with, $.keyword_values)),
          ),
        ),
        $._column_key_constraint,
        seq(
          $.keyword_index,
          field('name', $.identifier),
          optional($._constraint_index_kind),
          optional($.ordered_columns),
          optional($._constraint_index_options),
          optional($.on_filegroup),
          optional($._index_filestream),
        ),
        $.generated_always,
        seq($.keyword_column_set, $.keyword_for, $.keyword_all_sparse_columns),
        // MASKED WITH (FUNCTION = 'default()') — Dynamic Data Masking (2016).
        // `repeat($._column_constraint)` lets a column carry both this and
        // ENCRYPTED WITH below, which SQL Server itself rejects (a column can't
        // be both masked and Always Encrypted) — deliberate over-acceptance,
        // the same call this file already makes for `option`/`_column_constraint`'s
        // bare-identifier catch-all: a downstream linter judges the semantic
        // conflict, not the grammar.
        $.masked_with,
        // ENCRYPTED WITH (COLUMN_ENCRYPTION_KEY = k, ENCRYPTION_TYPE = ...,
        // ALGORITHM = 'x') — Always Encrypted (2016); the referenced key comes
        // from CREATE COLUMN ENCRYPTION KEY below.
        $.encrypted_with,
        alias($.keyword_filestream, $.identifier),
        alias($.keyword_sparse, $.identifier),
        alias($.keyword_rowguidcol, $.identifier),
        // Other bare column flags.
        $.identifier,
      ),
    ),

  _column_key_constraint: ($) =>
    prec.right(
      seq(
        optional(seq($.keyword_constraint, field('name', $.identifier))),
        $._column_key_constraint_body,
      ),
    ),

  _column_key_constraint_body: ($) =>
    prec.right(
      choice(
        seq($._primary_key, ...index_spec($, { columns: true, optionalColumns: true })),
        seq($.keyword_unique, ...index_spec($, { columns: true, optionalColumns: true })),
        seq(
          // SQL Server also takes the table form's column list here: a INT FOREIGN KEY (a) REFERENCES r (a).
          optional(seq($.keyword_foreign, $.keyword_key, optional(paren_list($.identifier, true)))),
          $.keyword_references,
          $.object_reference,
          optional(paren_list($.identifier, true)),
          repeat($.referential_action),
          optional($._not_for_replication),
        ),
        seq(
          $.keyword_check,
          optional($._not_for_replication),
          wrapped_in_parenthesis($._expression),
        ),
      ),
    ),

  generated_always: ($) =>
    seq(
      $.keyword_generated,
      $.keyword_always,
      $.keyword_as,
      choice($.keyword_row, $.keyword_transaction_id, $.keyword_sequence_number),
      choice($.keyword_start, $.keyword_end),
      optional($.keyword_hidden),
    ),

  _constraint_index_kind: ($) =>
    choice($._index_kind, $.keyword_hash, seq($.keyword_nonclustered, $.keyword_hash)),

  _constraint_index_options: ($) =>
    choice($.with_options, seq($.keyword_with, alias($._fillfactor_option, $.option))),
  _fillfactor_option: ($) =>
    seq(field('name', alias($.keyword_fillfactor, $.identifier)), '=', field('value', $.literal)),

  _index_filestream: ($) => seq(alias($.keyword_filestream_on, $.identifier), $._filegroup_name),

  masked_with: ($) => seq($.keyword_masked, $.with_options),

  encrypted_with: ($) => seq($.keyword_encrypted, $.with_options),

  _index_kind: ($) => choice($.keyword_clustered, $.keyword_nonclustered),

  // Table-level constraint, also the shape ALTER TABLE ... ADD accepts.
  constraint: ($) =>
    prec.right(
      seq(optional(seq($.keyword_constraint, field('name', $.identifier))), $._constraint_body),
    ),

  _constraint_body: ($) =>
    prec.right(
      choice(
        seq($._primary_key, ...index_spec($, { columns: true })),
        seq($.keyword_unique, ...index_spec($, { columns: true })),
        seq(
          $.keyword_foreign,
          $.keyword_key,
          paren_list($.identifier, true),
          $.keyword_references,
          $.object_reference,
          optional(paren_list($.identifier, true)),
          repeat($.referential_action),
          optional($._not_for_replication),
        ),
        seq(
          $.keyword_check,
          optional($._not_for_replication),
          wrapped_in_parenthesis($._expression),
        ),
        seq(
          $.keyword_connection,
          paren_list(seq($.object_reference, $.keyword_to, $.object_reference), true),
          optional(
            seq(
              $.keyword_on,
              $.keyword_delete,
              choice($.keyword_cascade, seq($.keyword_no, $.keyword_action)),
            ),
          ),
        ),
        // ALTER TABLE t ADD [CONSTRAINT name] DEFAULT expression FOR column
        seq(
          $.keyword_default,
          $._expression,
          $.keyword_for,
          field('column', $.identifier),
          optional(seq($.keyword_with, $.keyword_values)),
        ),
        // Inline rowstore, hash and columnstore indexes share placement/options.
        seq(
          $.keyword_index,
          field('name', $.identifier),
          optional($.keyword_unique),
          choice(
            seq(
              $.keyword_clustered,
              $.keyword_columnstore,
              optional(seq($.keyword_order, paren_list($.identifier, true))),
            ),
            seq($.keyword_nonclustered, $.keyword_columnstore, $.ordered_columns),
            seq(optional($._constraint_index_kind), $.ordered_columns),
          ),
          optional($.covering_columns),
          optional($.where),
          optional($.with_options),
          optional($.on_filegroup),
          optional($._index_filestream),
        ),
      ),
    ),

  // ON { DELETE | UPDATE } { NO ACTION | CASCADE | SET NULL | SET DEFAULT }
  referential_action: ($) =>
    seq(
      $.keyword_on,
      choice($.keyword_delete, $.keyword_update),
      choice(
        seq($.keyword_no, $.keyword_action),
        $.keyword_cascade,
        seq($.keyword_set, choice($.keyword_null, $.keyword_default)),
      ),
    ),

  ordered_columns: ($) => paren_list(alias($.ordered_column, $.column), true),

  ordered_column: ($) => seq(field('name', $._column), optional($.direction)),

  // Filegroup strings are accepted with either QUOTED_IDENTIFIER setting (levels 100-170).
  _filegroup_name: ($) => choice($.identifier, alias($._single_quote_string, $.literal)),

  // ON filegroup | ON partition_scheme (column) | ON "default"
  on_filegroup: ($) =>
    prec.right(
      seq($.keyword_on, $._filegroup_name, optional(wrapped_in_parenthesis($.identifier))),
    ),

  // The option list most DDL statements take: `WITH (PAD_INDEX = OFF,
  // FILLFACTOR = 90)`. Option names are identifiers — there are hundreds
  // and nothing downstream needs them distinguished.
  with_options: ($) =>
    seq(
      $.keyword_with,
      paren_list(
        choice(
          $.option,
          alias($._compression_partitions, $.option),
          alias($._move_to_option, $.option),
        ),
        true,
      ),
    ),

  // shortcut: the shared suffix accepts any option name; add per-option applicability in ADR-0003.
  _compression_partitions: ($) =>
    seq(
      field('name', $.identifier),
      '=',
      field('value', $._option_value),
      $.keyword_on,
      $.keyword_partitions,
      paren_list($.truncate_partition_range, true),
    ),

  _move_to_option: ($) =>
    seq(
      field('name', alias($.keyword_move, $.identifier)),
      $.keyword_to,
      field('value', $._filegroup_name),
      optional(wrapped_in_parenthesis($.identifier)),
    ),

  // The unparenthesized spelling security and server objects use:
  // `WITH DEFAULT_DATABASE = [master], CHECK_POLICY = OFF`. Kept separate
  // from with_options because inside a comma-separated column list the
  // bare form is ambiguous with the next list item.
  with_clause: ($) => prec.right(seq($.keyword_with, comma_list($.option, true))),

  option: ($) =>
    prec.right(
      choice(
        seq(
          field('name', $.identifier),
          optional(
            choice(
              seq('=', field('value', $._option_value)),
              field('value', $._option_value),
              // FORMAT_OPTIONS (FIELD_TERMINATOR = '|'), WAIT_AT_LOW_PRIORITY (ABORT_AFTER_WAIT = SELF)
              paren_list($.option, true),
            ),
          ),
        ),
        seq(
          field('name', alias($.keyword_filter_predicate, $.identifier)),
          '=',
          field('value', choice($.invocation, $.literal)),
        ),
      ),
    ),

  _qualified_option_name: ($) =>
    choice(
      seq(field('schema', $.identifier), '.', field('name', $.identifier)),
      seq(
        field('database', $.identifier),
        '.',
        field('schema', $.identifier),
        '.',
        field('name', $.identifier),
      ),
    ),

  _option_value: ($) =>
    prec.right(
      choice(
        // ON, OFF, PAGE, ..., optionally with sub-options:
        // ONLINE = ON (WAIT_AT_LOW_PRIORITY (...)), QUERY_STORE = ON (OPERATION_MODE = READ_WRITE)
        seq(
          choice(
            $.keyword_on,
            $.keyword_off,
            $.identifier,
            alias($._qualified_option_name, $.object_reference),
          ),
          optional(paren_list($.option, true)),
        ),
        $.keyword_default,
        // 100 MB, 50 PERCENT
        seq($.literal, optional($.identifier)),
      ),
    ),
};
