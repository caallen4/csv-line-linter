# csvlint

A CSV file will happily accept a row with the wrong number of fields, a
quote that never closes, or a stray `"` in the middle of a plain text field.
Most tools that read CSV either silently misalign the columns or throw an
opaque parse error with no line number. I got tired of finding out about
these problems downstream, in whatever spreadsheet or import job happened to
choke on them, so this checks the file itself first.

It reads a CSV file, parses it properly (quoted fields, embedded commas,
embedded newlines, `""` as an escaped quote), and prints every structural
problem it finds along with the line it starts on.

## Usage

```
$ cargo run -- orders.csv
orders.csv:14: ragged-row: row has 3 fields but header has 4
orders.csv:47: unclosed-quote: quoted field is never closed before end of file
```

No output and exit code 0 means the file is clean. Any finding sets the
exit code to 1, so it can be dropped into a pre-commit hook or CI step -
except `stray-quote`, which is reported but does not fail the run on its
own, since it is often a false alarm (an apostrophe, a name with a quote
mark in it) rather than an actually broken row. Pass `--strict` to make it
fatal too:

```
$ cargo run -- --strict orders.csv
```

Pass `--json` to get findings as a single JSON array on stdout instead of
one line of text per finding - useful for a CI step that wants to parse the
output rather than grep it:

```
$ cargo run -- --json orders.csv
[{"file":"orders.csv","line":14,"rule":"ragged-row","severity":"error","message":"row has 3 fields but header has 4"}]
```

`--json` and `--strict` combine normally: `--strict` still controls the
exit code, it just doesn't change the shape of the JSON output.

Build a release binary the normal way:

```
$ cargo build --release
$ ./target/release/csvlint orders.csv other.csv
```

Leave off the file argument, or pass `-`, to read from stdin instead - for
piping in the output of another command:

```
$ curl -s https://example.com/export.csv | csvlint
stdin:14: ragged-row: row has 3 fields but header has 4
```

## What it checks right now

- `ragged-row` - a row has a different number of fields than the header row.
- `unclosed-quote` - a quoted field is still open when the file ends.
- `stray-quote` - a `"` shows up outside of a quoted field, which almost
  always means a text field wasn't quoted the way it should have been.
  Warning-level: shown but not fatal unless `--strict` is passed.
- `duplicate-column` - two or more header columns share the same name,
  which usually means whatever reads this file by column name will only
  ever see one of them.
- `trailing-whitespace` - an unquoted field has a trailing space or tab
  right before the delimiter or end of line. Whitespace inside a quoted
  field is left alone, since quoting is how CSV says to keep it on purpose.

Line numbers point at the line a row *starts* on. A quoted field can span
several lines (it's legal for a field to contain a literal newline), and
the reported line is where that row began, not wherever the parser happened
to be when it noticed a problem.

A completely blank line is ignored rather than counted as a one-column row.
That matches what every CSV export I've actually run into means by a blank
line - trailing newlines from an editor or a spreadsheet's "extra row at the
end" habit, not real data.

## Development

No dependencies, standard library only. Tests are table-driven in
`src/lint.rs`, covering the same awkward cases described above (embedded
newlines, escaped quotes, CRLF, missing trailing newline, blank lines).

```
$ cargo test
```
