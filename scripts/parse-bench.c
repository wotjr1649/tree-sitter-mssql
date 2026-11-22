#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tree_sitter/api.h>

#ifdef PARSE_BENCH_CALLGRIND
#include <valgrind/callgrind.h>
#else
#define CALLGRIND_TOGGLE_COLLECT ((void)0)
#endif

const TSLanguage *tree_sitter_mssql(void);

static TSInputEdit insert_space(char *source, uint32_t length) {
  TSPoint start = {0, 0};
  uint32_t offset = 0;
  // Insert at the start of the line containing the byte midpoint.
  for (uint32_t i = 0; i < length / 2; i++) {
    if (source[i] == '\n') {
      start.row++;
      offset = i + 1;
    }
  }
  memmove(source + offset + 1, source + offset, length - offset);
  source[offset] = ' ';
  return (TSInputEdit){
    .start_byte = offset,
    .old_end_byte = offset,
    .new_end_byte = offset + 1,
    .start_point = start,
    .old_end_point = start,
    .new_end_point = {start.row, 1},
  };
}

int main(int argc, char **argv) {
  if (argc != 3 || (strcmp(argv[2], "fresh") && strcmp(argv[2], "incremental"))) {
    fprintf(stderr, "usage: parse-bench INPUT.bin fresh|incremental\n");
    return 2;
  }
  bool incremental = strcmp(argv[2], "incremental") == 0;
  FILE *input = fopen(argv[1], "rb");
  if (!input) {
    perror(argv[1]);
    return 2;
  }

  uint64_t files = 0, bytes = 0, errors = 0;
  unsigned char header[4];
  size_t size;
  while ((size = fread(header, 1, sizeof header, input)) != 0) {
    if (size != sizeof header) {
      fprintf(stderr, "truncated length in %s\n", argv[1]);
      return 2;
    }
    uint32_t length = (uint32_t)header[0] | (uint32_t)header[1] << 8 |
                      (uint32_t)header[2] << 16 | (uint32_t)header[3] << 24;
    if (length == UINT32_MAX) {
      fprintf(stderr, "input too large in %s\n", argv[1]);
      return 2;
    }
    char *source = malloc((size_t)length + 1);
    if (!source || fread(source, 1, length, input) != length) {
      fprintf(stderr, "cannot load input %" PRIu64 " from %s\n", files + 1, argv[1]);
      return 2;
    }

    TSParser *parser = ts_parser_new();
    if (!parser || !ts_parser_set_language(parser, tree_sitter_mssql())) {
      fprintf(stderr, "cannot initialize parser with this grammar ABI\n");
      return 2;
    }
    TSTree *old = NULL;
    TSInputEdit edit;
    if (incremental) {
      old = ts_parser_parse_string(parser, NULL, source, length);
      if (!old) {
        fprintf(stderr, "initial parse failed for input %" PRIu64 "\n", files + 1);
        return 2;
      }
      edit = insert_space(source, length);
    }

    CALLGRIND_TOGGLE_COLLECT;
    if (incremental) ts_tree_edit(old, &edit);
    TSTree *tree = ts_parser_parse_string(parser, old, source, length + incremental);
    CALLGRIND_TOGGLE_COLLECT;

    if (!tree) {
      fprintf(stderr, "measured parse failed for input %" PRIu64 "\n", files + 1);
      return 2;
    }
    files++;
    bytes += length;
    errors += ts_node_has_error(ts_tree_root_node(tree));
    ts_tree_delete(tree);
    if (old) ts_tree_delete(old);
    ts_parser_delete(parser);
    free(source);
  }

  if (ferror(input) || files == 0) {
    fprintf(stderr, "empty or unreadable input pack %s\n", argv[1]);
    return 2;
  }
  fclose(input);
  printf("%" PRIu64 " %" PRIu64 " %" PRIu64 "\n", files, bytes, errors);
  return 0;
}
