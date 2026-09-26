# tablefmt

Re-aligns the pipe tables in a Markdown document. Plain Ruby, no gems; it
must run on Ruby 2.6 and on Ruby 3. `lib/tablefmt.rb` defines
`Tablefmt.format(text) -> String`.

A **table** is a header row, a delimiter row, then any number of body rows,
on consecutive lines. A row is a line that starts with `|` once leading
spaces are ignored; the delimiter row's cells are each `-{1,}` with an
optional `:` at either end (`---`, `:--`, `--:`, `:-:`). Anything else,
including everything inside fenced code blocks (between lines starting with
three backticks), is copied unchanged.

Formatting a table:

- Cells are split on `|` except an escaped `\|`, which stays as it is, and
  trimmed. The leading and trailing `|` of a row are optional in the input
  and always present in the output.
- The table has as many columns as its widest row; shorter rows get empty
  cells. A column is as wide as its widest cell (`String#length`), and at
  least 3.
- Each row is written `| cell | cell |`, one space of padding on each side of
  a cell. Cells align per the delimiter: left (`---` or `:--`) pads on the
  right, right (`--:`) on the left, center (`:-:`) on both sides with any odd
  space on the right.
- The delimiter row keeps each column's alignment marks and fills the column
  width with dashes: `---`, `:--`, `--:` or `:-:` stretched, e.g.
  `:------:`. A column the delimiter row did not reach is left-aligned (`---`).
- Indentation before a table's first `|` is dropped.

The text around tables, trailing newline included, is unchanged.

Run the tests with `ruby scripts/test.rb`.
