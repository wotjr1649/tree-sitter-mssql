import { comma_list, paren_list, write_target } from '../helpers.js';

// Rules shared by more than one DML writer. `output_clause` is used by
// INSERT, UPDATE, DELETE and MERGE; `assignment`/`_set_values` are used by
// UPDATE and MERGE's `WHEN MATCHED THEN UPDATE SET` action, but not INSERT.
export default {
  // DELETE and UPDATE share these nonempty suffixes, including their clause order.
  _dml_write_tail: ($) =>
    prec.right(
      choice(seq($.output_clause, optional($._dml_write_after_output)), $._dml_write_after_output),
    ),
  _dml_write_after_output: ($) =>
    prec.right(choice(seq($.from, optional($._dml_write_after_from)), $._dml_write_after_from)),
  _dml_write_after_from: ($) =>
    prec.right(choice(seq($.where, optional($.option_clause)), $.option_clause)),

  _rowset_write_target: ($) => prec.right(write_target($, { allowRowsetFunction: true })),

  // OUTPUT term [, ...] [INTO table [(columns)] [OUTPUT term [, ...]]] — on INSERT, UPDATE, DELETE
  // and MERGE. The terms reference the `inserted`/`deleted` pseudo-tables
  // and `$action`, all of which are ordinary identifiers here.
  output_clause: ($) =>
    seq(
      $.keyword_output,
      comma_list($.term, true),
      optional(
        seq(
          $.keyword_into,
          $.object_reference,
          optional(paren_list($.identifier, true)),
          optional(seq($.keyword_output, comma_list($.term, true))),
        ),
      ),
    ),

  assignment: ($) =>
    seq(
      field('left', alias($._qualified_field, $.field)),
      $.assignment_operator,
      field('right', choice($._expression, $.keyword_default, $.assignment)),
    ),

  // An item is an assignment or a bare method call with no `=`, which is
  // how the xml and json `.modify()` mutators are applied to a column:
  //   UPDATE t SET x.modify('insert ...')
  _set_values: ($) => seq($.keyword_set, comma_list(choice($.assignment, $.invocation), true)),
};
