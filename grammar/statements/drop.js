import { comma_list } from '../helpers.js';

// DROP <object kind> [IF EXISTS] name [, ...]. T-SQL has no CASCADE or
// RESTRICT on any DROP.
export default {
  _drop_statement: ($) =>
    choice(
      $.drop_table,
      $.drop_aggregate,
      $.drop_default,
      $.drop_rule,
      $.drop_application_role,
      $.drop_broker_priority,
      $.drop_contract,
      $.drop_credential,
      $.drop_cryptographic_provider,
      $.drop_database_audit_specification,
      $.drop_database_encryption_key,
      $.drop_message_type,
      $.drop_partition_function,
      $.drop_partition_scheme,
      $.drop_queue,
      $.drop_remote_service_binding,
      $.drop_resource_pool,
      $.drop_route,
      $.drop_search_property_list,
      $.drop_server_audit,
      $.drop_server_audit_specification,
      $.drop_service,
      $.drop_workload_group,

      $.drop_endpoint,
      $.drop_external_data_source,
      $.drop_external_model,
      $.drop_external_language,
      $.drop_external_library,
      $.drop_external_resource_pool,
      $.drop_external_file_format,
      $.drop_external_table,
      $.drop_fulltext_catalog,
      $.drop_fulltext_index,
      $.drop_fulltext_stoplist,
      $.drop_view,
      $.drop_index,
      $.drop_procedure,
      $.drop_function,
      $.drop_trigger,
      $.drop_type,
      $.drop_schema,
      $.drop_database,
      $.drop_role,
      $.drop_user,
      $.drop_login,
      $.drop_synonym,
      $.drop_sequence,
      $.drop_statistics,
      $.drop_assembly,
      $.drop_event_session,
      $.drop_master_key,
      $.drop_certificate,
      $.drop_symmetric_key,
      $.drop_asymmetric_key,
      $.drop_column_encryption_key,
      $.drop_column_master_key,
      $.drop_security_policy,
      $.drop_xml_schema_collection,
    ),

  // DROP XML SCHEMA COLLECTION [IF EXISTS] name
  drop_xml_schema_collection: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_xml,
      $.keyword_schema,
      $.keyword_collection,
      optional($._if_exists),
      field('name', $.object_reference),
    ),

  drop_table: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_table,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  drop_view: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_view,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  // DROP INDEX [IF EXISTS] name ON table [, ...]
  // DROP INDEX table.name                        (legacy spelling)
  drop_index: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_index,
      optional($._if_exists),
      comma_list(
        seq(field('name', $.object_reference), optional(seq($.keyword_on, $.object_reference))),
        true,
      ),
    ),

  drop_procedure: ($) =>
    seq(
      $.keyword_drop,
      choice($.keyword_procedure, $.keyword_proc),
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  drop_function: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_function,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  // DROP TRIGGER [IF EXISTS] name [, ...] [ON DATABASE | ON ALL SERVER]
  drop_trigger: ($) =>
    prec.right(
      seq(
        $.keyword_drop,
        $.keyword_trigger,
        optional($._if_exists),
        comma_list(field('name', $.object_reference), true),
        optional(
          seq($.keyword_on, choice($.keyword_database, seq($.keyword_all, $.keyword_server))),
        ),
      ),
    ),

  drop_type: ($) =>
    seq($.keyword_drop, $.keyword_type, optional($._if_exists), field('name', $.object_reference)),

  drop_schema: ($) =>
    seq($.keyword_drop, $.keyword_schema, optional($._if_exists), field('name', $.identifier)),

  drop_database: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_database,
      optional($._if_exists),
      comma_list(field('name', $.identifier), true),
    ),

  drop_role: ($) =>
    seq(
      $.keyword_drop,
      optional($.keyword_server),
      $.keyword_role,
      optional($._if_exists),
      field('name', $.identifier),
    ),

  drop_user: ($) =>
    seq($.keyword_drop, $.keyword_user, optional($._if_exists), field('name', $.identifier)),

  drop_login: ($) => seq($.keyword_drop, $.keyword_login, field('name', $.identifier)),

  drop_synonym: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_synonym,
      optional($._if_exists),
      field('name', $.object_reference),
    ),

  drop_sequence: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_sequence,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  drop_statistics: ($) =>
    seq($.keyword_drop, $.keyword_statistics, comma_list(field('name', $.object_reference), true)),

  drop_assembly: ($) =>
    prec.right(
      seq(
        $.keyword_drop,
        $.keyword_assembly,
        optional($._if_exists),
        comma_list(field('name', $.identifier), true),
        optional(seq($.keyword_with, $.keyword_no, $.identifier)),
      ),
    ),

  // DROP EVENT SESSION name ON SERVER
  drop_event_session: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_event,
      $.keyword_session,
      optional($._if_exists),
      field('name', $.identifier),
      $.keyword_on,
      $.keyword_server,
    ),
  drop_aggregate: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_aggregate,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  drop_default: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_default,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  drop_rule: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_rule,
      optional($._if_exists),
      comma_list(field('name', $.object_reference), true),
    ),

  drop_application_role: ($) =>
    seq($.keyword_drop, $.keyword_application, $.keyword_role, field('name', $.identifier)),

  drop_broker_priority: ($) =>
    seq($.keyword_drop, $.keyword_broker, $.keyword_priority, field('name', $.identifier)),

  drop_contract: ($) => seq($.keyword_drop, $.keyword_contract, field('name', $.identifier)),

  drop_credential: ($) =>
    seq(
      $.keyword_drop,
      optional(seq($.keyword_database, $.keyword_scoped)),
      $.keyword_credential,
      field('name', $.identifier),
    ),

  drop_cryptographic_provider: ($) =>
    seq($.keyword_drop, $.keyword_cryptographic, $.keyword_provider, field('name', $.identifier)),

  drop_database_audit_specification: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_database,
      $.keyword_audit,
      $.keyword_specification,
      field('name', $.identifier),
    ),

  drop_database_encryption_key: ($) =>
    seq($.keyword_drop, $.keyword_database, $.keyword_encryption, $.keyword_key),

  drop_message_type: ($) =>
    seq($.keyword_drop, $.keyword_message, $.keyword_type, field('name', $.identifier)),

  drop_partition_function: ($) =>
    seq($.keyword_drop, $.keyword_partition, $.keyword_function, field('name', $.identifier)),

  drop_partition_scheme: ($) =>
    seq($.keyword_drop, $.keyword_partition, $.keyword_scheme, field('name', $.identifier)),

  drop_queue: ($) => seq($.keyword_drop, $.keyword_queue, field('name', $.object_reference)),

  drop_remote_service_binding: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_remote,
      $.keyword_service,
      $.keyword_binding,
      field('name', $.identifier),
    ),

  drop_resource_pool: ($) =>
    seq($.keyword_drop, $.keyword_resource, $.keyword_pool, field('name', $.identifier)),

  drop_route: ($) => seq($.keyword_drop, $.keyword_route, field('name', $.identifier)),

  drop_search_property_list: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_search,
      $.keyword_property,
      $.keyword_list,
      field('name', $.identifier),
    ),

  drop_server_audit: ($) =>
    seq($.keyword_drop, $.keyword_server, $.keyword_audit, field('name', $.identifier)),

  drop_server_audit_specification: ($) =>
    seq(
      $.keyword_drop,
      $.keyword_server,
      $.keyword_audit,
      $.keyword_specification,
      field('name', $.identifier),
    ),

  drop_service: ($) => seq($.keyword_drop, $.keyword_service, field('name', $.identifier)),

  drop_workload_group: ($) =>
    seq($.keyword_drop, $.keyword_workload, $.keyword_group, field('name', $.identifier)),
};
