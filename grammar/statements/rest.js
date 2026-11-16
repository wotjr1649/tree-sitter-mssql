import { comma_list } from '../helpers.js';

export default {
  sensitivity_classification_statement: ($) =>
    seq(
      choice(
        seq(
          $.keyword_add,
          $.keyword_sensitivity,
          $.keyword_classification,
          $.keyword_to,
          comma_list($.object_reference, true),
          $.with_options,
        ),
        seq(
          $.keyword_drop,
          $.keyword_sensitivity,
          $.keyword_classification,
          $.keyword_from,
          comma_list($.object_reference, true),
        ),
      ),
    ),
  signature_statement: ($) =>
    seq(
      choice(
        seq($.keyword_add, optional($.keyword_counter), $.keyword_signature, $.keyword_to),
        seq($.keyword_drop, optional($.keyword_counter), $.keyword_signature, $.keyword_from),
      ),
      optional(seq($.keyword_object, '::')),
      $.object_reference,
      $.keyword_by,
      comma_list($.signature_signer, true),
    ),
  _signature_with_clause: ($) => seq($.keyword_with, $.option),

  signature_signer: ($) =>
    seq(
      choice(
        $.keyword_certificate,
        seq(choice($.keyword_asymmetric, $.keyword_symmetric), $.keyword_key),
      ),
      $.identifier,
      optional(alias($._signature_with_clause, $.with_clause)),
    ),
};
