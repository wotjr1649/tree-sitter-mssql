import { make_keyword, paren_list, wrapped_in_parenthesis } from '../helpers.js';

export default {
  create_endpoint: ($) =>
    seq(
      $.keyword_create,
      $.keyword_endpoint,
      field('name', $.identifier),
      optional(seq($.keyword_authorization, $.identifier)),
      optional($.endpoint_state),
      $.endpoint_transport,
      $.endpoint_payload,
    ),

  alter_endpoint: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_endpoint,
      field('name', $.identifier),
      choice(
        seq($.endpoint_state, optional($.endpoint_transport), optional($.endpoint_payload)),
        seq($.endpoint_transport, optional($.endpoint_payload)),
        $.endpoint_payload,
      ),
    ),

  drop_endpoint: ($) => seq($.keyword_drop, $.keyword_endpoint, field('name', $.identifier)),

  endpoint_state: ($) =>
    seq(
      alias($._endpoint_state, $.identifier),
      '=',
      field('value', alias($._endpoint_state_value, $.identifier)),
    ),

  // Uniqueness and CREATE requirements are checked by tree-check (ADR-0003).
  endpoint_transport: ($) =>
    seq($.keyword_as, alias($._endpoint_tcp, $.identifier), paren_list($._endpoint_tcp_option)),

  _endpoint_tcp_option: ($) =>
    choice(
      alias($._endpoint_assignment, $.option),
      alias($._endpoint_listener_ip_option, $.option),
    ),

  _endpoint_listener_ip_option: ($) =>
    seq(
      field('name', alias($._endpoint_listener_ip, $.identifier)),
      '=',
      choice(
        field('value', $.keyword_all),
        wrapped_in_parenthesis(
          field('value', choice(alias($._endpoint_ipv4, $.literal), $.literal)),
        ),
      ),
    ),

  endpoint_payload: ($) =>
    seq(
      $.keyword_for,
      field('type', alias($._endpoint_payload_type, $.identifier)),
      paren_list($._endpoint_payload_option),
    ),

  _endpoint_payload_option: ($) =>
    choice(
      alias($._endpoint_assignment, $.option),
      alias($._endpoint_authentication_option, $.option),
      alias($._endpoint_encryption_option, $.option),
      alias($._endpoint_role_option, $.option),
    ),

  _endpoint_authentication_option: ($) =>
    seq(
      field('name', alias($._endpoint_authentication, $.identifier)),
      '=',
      choice(
        seq($._endpoint_windows_authentication, optional($._endpoint_certificate_authentication)),
        seq($._endpoint_certificate_authentication, optional($._endpoint_windows_authentication)),
      ),
    ),

  _endpoint_windows_authentication: ($) =>
    prec.right(seq($.keyword_windows, optional(field('protocol', $.identifier)))),

  _endpoint_certificate_authentication: ($) =>
    seq($.keyword_certificate, field('certificate', $.identifier)),

  _endpoint_encryption_option: ($) =>
    prec.right(
      seq(
        field('name', alias($.keyword_encryption, $.identifier)),
        '=',
        field('value', $.identifier),
        optional(
          seq(
            alias($._endpoint_algorithm, $.identifier),
            field('algorithm', $.identifier),
            optional(field('algorithm', $.identifier)),
          ),
        ),
      ),
    ),

  _endpoint_role_option: ($) =>
    seq(
      field('name', alias($.keyword_role, $.identifier)),
      '=',
      field('value', choice($.identifier, $.keyword_all)),
    ),

  _endpoint_assignment: ($) =>
    seq(field('name', $.identifier), '=', field('value', $._option_value)),

  _endpoint_state_value: (_) =>
    token(
      prec(1, choice(make_keyword('started'), make_keyword('stopped'), make_keyword('disabled'))),
    ),

  _endpoint_state: (_) => token(prec(1, make_keyword('state'))),
  _endpoint_tcp: (_) => token(prec(1, make_keyword('tcp'))),
  _endpoint_payload_type: (_) =>
    token(
      prec(
        1,
        choice(
          make_keyword('tsql'),
          make_keyword('service_broker'),
          make_keyword('database_mirroring'),
        ),
      ),
    ),
  _endpoint_listener_ip: (_) => token(prec(1, make_keyword('listener_ip'))),
  _endpoint_ipv4: (_) => /[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+/,
  _endpoint_authentication: (_) => token(prec(1, make_keyword('authentication'))),
  _endpoint_algorithm: (_) => token(prec(1, make_keyword('algorithm'))),
};
