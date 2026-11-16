import { comma_list, paren_list, wrapped_in_parenthesis } from '../helpers.js';

// Database-only options keep the shared table/index option contract untouched.
export default {
  _database_name: ($) =>
    choice(
      $.identifier,
      alias($.keyword_scoped, $.identifier),
      alias($.keyword_encryption, $.identifier),
      alias($.keyword_audit, $.identifier),
    ),
  database_option: ($) =>
    prec.right(
      seq(
        field('name', choice($.identifier, alias($.keyword_query_store, $.identifier))),
        optional(
          choice(
            seq(
              '=',
              choice(
                field('value', $._database_option_value),
                paren_list(alias($.database_option, $.option), true),
              ),
            ),
            field('value', $._database_option_value),
            paren_list(alias($.database_option, $.option), true),
          ),
        ),
      ),
    ),
  _database_option_value: ($) =>
    prec.right(
      choice(
        seq(
          choice(
            $.keyword_on,
            $.keyword_off,
            $.identifier,
            $.keyword_all,
            alias($.keyword_primary, $.identifier),
            alias($.keyword_full, $.identifier),
          ),
          optional(paren_list(alias($.database_option, $.option), true)),
        ),
        $.keyword_default,
        seq($.literal, optional(choice($.identifier, '%'))),
      ),
    ),
  database_file: ($) => paren_list(alias($.database_option, $.option), true),
  database_filegroup: ($) =>
    seq(
      $.keyword_filegroup,
      field('name', $.identifier),
      optional(
        seq($.keyword_contains, choice($.keyword_filestream, $.keyword_memory_optimized_data)),
      ),
      optional($.keyword_default),
      comma_list($.database_file, true),
    ),
  _database_files: ($) =>
    seq(
      $.keyword_on,
      comma_list(
        choice(seq(optional($.keyword_primary), $.database_file), $.database_filegroup),
        true,
      ),
    ),
  _alter_database_file: ($) =>
    choice(
      seq(
        $.keyword_add,
        optional($.keyword_log),
        $.keyword_file,
        comma_list($.database_file, true),
        optional(seq($.keyword_to, $.keyword_filegroup, $.identifier)),
      ),
      seq(
        $.keyword_add,
        $.keyword_filegroup,
        $.identifier,
        optional(
          seq($.keyword_contains, choice($.keyword_filestream, $.keyword_memory_optimized_data)),
        ),
      ),
      seq($.keyword_remove, choice($.keyword_file, $.keyword_filegroup), $.identifier),
      seq($.keyword_modify, $.keyword_file, $.database_file),
      seq(
        $.keyword_modify,
        $.keyword_filegroup,
        $.identifier,
        choice(
          $.keyword_default,
          $.keyword_readonly,
          $.identifier,
          seq($.keyword_name, '=', $.identifier),
        ),
      ),
    ),

  database_clear_option: ($) =>
    seq(
      field('name', alias($.keyword_query_store, $.identifier)),
      field('value', alias($.keyword_clear, $.identifier)),
      optional($.keyword_all),
    ),
  database_mirroring_option: ($) =>
    choice(
      seq(
        field('name', alias($.keyword_hadr, $.identifier)),
        choice(
          seq($.keyword_availability, $.keyword_group, '=', $.identifier),
          field(
            'value',
            choice(
              $.keyword_off,
              alias($.keyword_suspend, $.identifier),
              alias($.keyword_resume, $.identifier),
            ),
          ),
        ),
      ),
      seq(
        field('name', alias($.keyword_partner, $.identifier)),
        choice(
          seq('=', field('value', $.literal)),
          field(
            'value',
            choice(
              $.keyword_off,
              alias($.keyword_resume, $.identifier),
              alias($.keyword_suspend, $.identifier),
              alias($.keyword_failover, $.identifier),
              alias($.keyword_force_service_allow_data_loss, $.identifier),
            ),
          ),
          seq($.keyword_safety, choice($.keyword_full, $.keyword_off)),
          seq($.keyword_timeout, $.literal),
        ),
      ),
      seq(
        field('name', alias($.keyword_witness, $.identifier)),
        choice(seq('=', field('value', $.literal)), field('value', $.keyword_off)),
      ),
    ),
  create_database_encryption_key: ($) =>
    seq(
      $.keyword_create,
      $.keyword_database,
      $.keyword_encryption,
      $.keyword_key,
      $.with_clause,
      $._database_encryptor,
    ),
  alter_database_encryption_key: ($) =>
    seq(
      $.keyword_alter,
      $.keyword_database,
      $.keyword_encryption,
      $.keyword_key,
      choice(seq($.keyword_regenerate, $.with_clause), $._database_encryptor),
    ),
  _database_encryptor: ($) =>
    seq(
      $.keyword_encryption,
      $.keyword_by,
      $.keyword_server,
      choice($.keyword_certificate, seq($.keyword_asymmetric, $.keyword_key)),
      $.identifier,
    ),
  create_database_audit_specification: ($) =>
    seq($.keyword_create, $.keyword_database, $._create_audit_specification),
  alter_database_audit_specification: ($) =>
    seq($.keyword_alter, $.keyword_database, $._alter_audit_specification),
  create_server_audit_specification: ($) =>
    seq($.keyword_create, $.keyword_server, $._create_audit_specification),
  alter_server_audit_specification: ($) =>
    seq($.keyword_alter, $.keyword_server, $._alter_audit_specification),
  _create_audit_specification: ($) =>
    prec.right(
      seq(
        $.keyword_audit,
        $.keyword_specification,
        field('name', $.identifier),
        $._audit_specification_target,
        optional(comma_list(seq($.keyword_add, $.audit_action), true)),
        optional($.with_options),
      ),
    ),
  _alter_audit_specification: ($) =>
    prec.right(
      seq(
        $.keyword_audit,
        $.keyword_specification,
        field('name', $.identifier),
        choice(
          seq(
            $._audit_specification_target,
            optional(comma_list(seq(choice($.keyword_add, $.keyword_drop), $.audit_action), true)),
            optional($.with_options),
          ),
          seq(
            comma_list(seq(choice($.keyword_add, $.keyword_drop), $.audit_action), true),
            optional($.with_options),
          ),
          $.with_options,
        ),
      ),
    ),
  _audit_specification_target: ($) =>
    seq($.keyword_for, $.keyword_server, $.keyword_audit, $.identifier),
  audit_action: ($) =>
    seq(
      '(',
      comma_list($.permission, true),
      optional(
        seq(
          $.keyword_on,
          optional(seq($.identifier, '::')),
          $.object_reference,
          $.keyword_by,
          comma_list($.identifier, true),
        ),
      ),
      ')',
    ),
  availability_option: ($) =>
    prec.right(
      seq(
        field('name', $.identifier),
        optional(
          choice(
            seq('=', choice(field('value', $._database_option_value), $._availability_routing)),
            field('value', $._database_option_value),
            paren_list(alias($.availability_option, $.option), true),
          ),
        ),
      ),
    ),
  _availability_routing: ($) => paren_list(choice($.literal, $._availability_routing), true),
  availability_replica: ($) =>
    seq($.literal, $.keyword_with, paren_list(alias($.availability_option, $.option), true)),
  _availability_group_header: ($) =>
    seq($.keyword_availability, $.keyword_group, field('name', $.identifier)),
  create_availability_group: ($) =>
    prec.right(
      seq(
        $.keyword_create,
        $._availability_group_header,
        optional($.with_options),
        choice(
          seq(
            $.keyword_for,
            optional(seq($.keyword_database, comma_list($.identifier, true))),
            $.keyword_replica,
            $.keyword_on,
            comma_list($.availability_replica, true),
          ),
          seq(
            $.keyword_availability,
            $.keyword_group,
            $.keyword_on,
            comma_list($.availability_replica, true),
          ),
        ),
        optional(seq($.keyword_listener, $.literal, $.availability_listener)),
      ),
    ),
  alter_availability_group: ($) =>
    prec.right(
      seq(
        $.keyword_alter,
        $._availability_group_header,
        choice(
          seq($.keyword_set, paren_list(alias($.availability_option, $.option), true)),
          seq(
            choice($.keyword_add, $.keyword_remove),
            $.keyword_database,
            comma_list($.identifier, true),
          ),
          seq(
            choice($.keyword_add, $.keyword_modify),
            $.keyword_replica,
            $.keyword_on,
            comma_list($.availability_replica, true),
          ),
          seq($.keyword_remove, $.keyword_replica, $.keyword_on, $.literal),
          seq(
            choice($.keyword_join, $.keyword_modify),
            $.keyword_availability,
            $.keyword_group,
            $.keyword_on,
            comma_list($.availability_replica, true),
          ),
          seq($.keyword_join, optional($.with_options)),
          seq(
            choice($.keyword_grant, $.keyword_deny),
            $.keyword_create,
            $.keyword_any,
            $.keyword_database,
          ),
          $.keyword_offline,
          $.keyword_failover,
          $.keyword_force_failover_allow_data_loss,
          seq($.keyword_add, $.keyword_listener, $.literal, $.availability_listener),
          seq(
            $.keyword_modify,
            $.keyword_listener,
            $.literal,
            wrapped_in_parenthesis(
              choice(
                seq(choice($.keyword_add, $.keyword_remove), $.keyword_ip, $.listener_ip),
                seq($.keyword_port, '=', $.literal),
              ),
            ),
          ),
          seq(choice($.keyword_remove, $.keyword_restart), $.keyword_listener, $.literal),
        ),
      ),
    ),
  listener_ip: ($) => wrapped_in_parenthesis(seq($.literal, optional(seq(',', $.literal)))),
  availability_listener: ($) =>
    seq(
      '(',
      $.keyword_with,
      choice(
        seq($.keyword_dhcp, optional(seq($.keyword_on, $.listener_ip))),
        seq($.keyword_ip, paren_list($.listener_ip, true)),
      ),
      optional(seq(',', $.keyword_port, '=', $.literal)),
      ')',
    ),
  alter_server_configuration: ($) =>
    prec.right(
      seq(
        $.keyword_alter,
        $.keyword_server,
        $.keyword_configuration,
        $.keyword_set,
        choice(
          seq(
            $.keyword_process,
            $.keyword_affinity,
            choice(
              seq($.keyword_cpu, '=', choice($.keyword_auto, comma_list($.server_cpu_range, true))),
              seq($.keyword_numanode, '=', comma_list($.server_cpu_range, true)),
            ),
          ),
          seq($.keyword_diagnostics, $.keyword_log, choice($.keyword_on, $.keyword_off, $.option)),
          seq($.keyword_failover, $.keyword_cluster, $.keyword_property, $.option),
          seq(
            $.keyword_hadr,
            $.keyword_cluster,
            $.keyword_context,
            '=',
            choice($.keyword_local, $.literal),
          ),
          seq(
            $.keyword_buffer,
            $.keyword_pool,
            $.keyword_extension,
            choice(seq($.keyword_on, $.server_buffer_pool_options), $.keyword_off),
          ),
          seq($.keyword_softnuma, choice($.keyword_on, $.keyword_off)),
          seq(
            $.keyword_memory_optimized,
            choice(
              $.keyword_on,
              $.keyword_off,
              seq(
                choice($.keyword_tempdb_metadata, $.keyword_hybrid_buffer_pool),
                '=',
                choice($.keyword_on, $.keyword_off),
                optional(paren_list($.option, true)),
              ),
            ),
          ),
          seq(
            $.keyword_suspend_for_snapshot_backup,
            '=',
            choice(
              seq($.keyword_on, optional(paren_list(alias($.database_option, $.option), true))),
              $.keyword_off,
            ),
          ),
        ),
      ),
    ),
  server_cpu_range: ($) => seq($.literal, optional(seq($.keyword_to, $.literal))),
  server_buffer_pool_options: ($) => paren_list($.option, true),
};
