import { comma_list, paren_list } from '../helpers.js';

export default {
  alter_external_data_source: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_external,
      $.keyword_data,
      $.keyword_source,
      field('name', $.identifier),
      $.keyword_set,
      comma_list($.option, true),
    ),

  create_external_model: ($) =>
    seq(
      $.keyword_create,
      $.keyword_external,
      $.keyword_model,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      $.with_options,
    ),

  alter_external_model: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_external,
      $.keyword_model,
      field('name', $.identifier),
      $.keyword_set,
      paren_list($.option, true),
    ),

  create_external_language: ($) =>
    seq(
      $.keyword_create,
      $.keyword_external,
      $.keyword_language,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      $.keyword_from,
      comma_list($.external_file_spec, true),
    ),

  alter_external_language: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_external,
      $.keyword_language,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      choice(
        seq(choice($.keyword_set, $.keyword_add), $.external_file_spec),
        seq($.keyword_remove, $.keyword_platform, $.identifier),
      ),
    ),

  create_external_library: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_external,
        $.keyword_library,
        field('name', $.identifier),
        optional(seq($.keyword_authorization, $.identifier)),
        $.keyword_from,
        comma_list($.external_file_spec, true),
        $.with_options,
      ),
    ),

  alter_external_library: ($) =>
    prec.right(
      seq(
        $.keyword_alter,
        $.keyword_external,
        $.keyword_library,
        field('name', $.identifier),
        optional(seq($.keyword_authorization, $.identifier)),
        $.keyword_set,
        $.external_file_spec,
        $.with_options,
      ),
    ),

  external_file_spec: ($) => paren_list($.option, true),

  create_external_resource_pool: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_external,
        $.keyword_resource,
        $.keyword_pool,
        field('name', $.identifier),
        optional(alias($._external_pool_options, $.with_options)),
      ),
    ),

  alter_external_resource_pool: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_external,
      $.keyword_resource,
      $.keyword_pool,
      field('name', $.identifier),
      alias($._external_pool_options, $.with_options),
    ),

  _external_pool_options: ($) =>
    seq(
      $.keyword_with,
      paren_list(choice($.option, alias($._external_pool_affinity, $.option)), true),
    ),

  _external_pool_affinity: ($) =>
    seq(
      field('name', alias($.keyword_affinity, $.identifier)),
      choice($.keyword_cpu, $.keyword_numanode),
      '=',
      choice($.identifier, paren_list($.external_cpu_range, true)),
    ),

  external_cpu_range: ($) => seq($.literal, optional(seq($.keyword_to, $.literal))),

  drop_external_data_source: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_external,
      $.keyword_data,
      $.keyword_source,
      field('name', $.identifier),
    ),

  drop_external_file_format: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_external,
      $.keyword_file,
      $.keyword_format,
      field('name', $.identifier),
    ),

  drop_external_language: ($) =>
    seq($.keyword_drop, $.keyword_external, $.keyword_language, field('name', $.identifier)),

  drop_external_library: ($) =>
    prec.right(
      seq(
        $.keyword_drop,
        $.keyword_external,
        $.keyword_library,
        field('name', $.identifier),
        optional(seq($.keyword_authorization, $.identifier)),
      ),
    ),

  drop_external_resource_pool: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_external,
      $.keyword_resource,
      $.keyword_pool,
      field('name', $.identifier),
    ),

  drop_external_table: ($) =>
    seq($.keyword_drop, $.keyword_external, $.keyword_table, field('name', $.object_reference)),

  drop_external_model: ($) =>
    seq($.keyword_drop, $.keyword_external, $.keyword_model, field('name', $.identifier)),
};
