import { comma_list, make_keyword, paren_list, wrapped_in_parenthesis } from '../helpers.js';

export default {
  _fulltext_population: (_) => token(prec(1, make_keyword('population'))),

  _fulltext_catalog_filegroup: ($) =>
    choice(
      $.identifier,
      wrapped_in_parenthesis(
        choice(
          seq($.identifier, ',', alias($.keyword_filegroup, $.identifier), $.identifier),
          seq(
            alias($.keyword_filegroup, $.identifier),
            $.identifier,
            optional(seq(',', $.identifier)),
          ),
        ),
      ),
    ),

  _fulltext_with_clause: ($) =>
    prec.right(seq($.keyword_with, comma_list($._fulltext_option, true))),

  _fulltext_with_options: ($) => seq($.keyword_with, paren_list($._fulltext_option, true)),

  _fulltext_option: ($) => choice($.option, alias($._fulltext_search_option, $.option)),

  _fulltext_search_option: ($) =>
    seq(
      field('name', alias($.keyword_search, $.identifier)),
      alias($.keyword_property, $.identifier),
      alias($.keyword_list, $.identifier),
      optional('='),
      field('value', $.identifier),
    ),

  create_fulltext_stoplist: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_fulltext,
        $.keyword_stoplist,
        field('name', $.identifier),
        optional(
          seq(
            $.keyword_from,
            choice(seq($.keyword_system, $.keyword_stoplist), $.object_reference),
          ),
        ),
        optional(seq($.keyword_authorization, $.identifier)),
      ),
    ),

  alter_fulltext_stoplist: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_fulltext,
      $.keyword_stoplist,
      field('name', $.identifier),
      choice(
        seq($.keyword_add, $.literal, $.keyword_language, $.literal),
        seq(
          $.keyword_drop,
          choice(
            seq($.literal, $.keyword_language, $.literal),
            seq($.keyword_all, optional(seq($.keyword_language, $.literal))),
          ),
        ),
      ),
    ),

  create_search_property_list: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $.keyword_search,
        $.keyword_property,
        $.keyword_list,
        field('name', $.identifier),
        optional(seq($.keyword_from, $.object_reference)),
        optional(seq($.keyword_authorization, $.identifier)),
      ),
    ),

  drop_fulltext_catalog: ($) =>
    seq($.keyword_drop, $.keyword_fulltext, $.keyword_catalog, field('name', $.identifier)),

  drop_fulltext_index: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_fulltext,
      $.keyword_index,
      $.keyword_on,
      field('name', $.object_reference),
    ),

  drop_fulltext_stoplist: ($) =>
    seq($.keyword_drop, $.keyword_fulltext, $.keyword_stoplist, field('name', $.identifier)),
};
