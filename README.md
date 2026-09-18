# csvsum

A small Rust CLI that reads one or more CSV files and prints a quick
exploratory-data-analysis summary for each column: type (numeric/text),
missing values, distinct-value count, numeric min/max/mean/stddev, and the
most frequent values.

## Build

```sh
make build          # debug build
make release        # optimized build
```

## Run

```sh
make run                                  # runs against examples/sample.csv
cargo run -- path/to/data.csv
cargo run -- data1.csv data2.csv          # multiple files
cargo run -- --delimiter ';' data.csv     # custom delimiter
cargo run -- --top 10 data.csv            # show top 10 frequent values per column
cargo run -- --no-header data.csv         # file has no header row
cargo run -- --clean data.csv             # also write a cleaned CSV
cargo run -- --clean --output out.csv data.csv   # choose the cleaned file's path
```

## Cleaning

Passing `--clean` produces a second, cleaned CSV alongside the summary:

- Any column that is **100% empty** (every row missing that field) is dropped
  entirely from the output.
- Any row that still has a missing field in a *kept* column is logged to
  stderr as a `WARNING: <file> row <n>: missing value in column(s): ...` and
  counted, but is still written to the cleaned file as-is.
- By default the output is written to `<name>.clean.<ext>` next to the input
  file; use `--output <path>` to choose the path explicitly (only valid with
  a single input file).
- A short summary (rows processed, rows with nulls, columns dropped/kept) is
  printed after the cleaned file is written.

```
$ cargo run -- --clean examples/wine-ratings.csv
...
Clean summary for examples/wine-ratings.csv
  output file:      examples/wine-ratings.clean.csv
  rows processed:   32780
  rows with nulls:  541 (1.7%)
  columns dropped:  1 (grape)
  columns kept:     6
```

Or after `make install`:

```sh
csvsum data.csv
```

## Example output

```
$ csvsum examples/sample.csv
File:    examples/sample.csv
Rows:    10
Columns: 6

── id (numeric)
   present: 10 / 10    missing: 0 (0.0%)    distinct: 10
   min: 1    max: 10    mean: 5.5    stddev: 2.872
   top: "1"×1, "10"×1, "2"×1, "3"×1, "4"×1

── age (numeric)
   present: 8 / 10    missing: 2 (20.0%)    distinct: 8
   min: 25    max: 52    mean: 34.875    stddev: 8.157
   ...
```

## Data drift

`csvsum drift` persists a summary that goes beyond the shape of the data
(`schema` covers shape/types) and captures its **distribution**: numeric
min/max/mean ranges, plus a categorical fingerprint (distinct-value count and
the values themselves) for text columns.

```sh
cargo run -- drift baseline data.csv                    # save data.drift.json
cargo run -- drift baseline data.csv --output base.json # choose the path
cargo run -- drift check data.csv --baseline base.json  # compare, exit 1 on drift
cargo run -- drift check data.csv --baseline base.json --mode warn  # exit 0, print WARNING
```

A `drift check` flags:

- **Numeric range drift**: a value falls below the baseline's min or above its
  max. Allow some slack with `--tolerance <percent>` (percentage of the
  baseline's range; default `0`).
- **Categorical cardinality drift**: the number of distinct values in a text
  column changes by more than `--category-tolerance <percent>` (default
  `20`) — e.g. `examples/wine-ratings.csv`'s `region` column going from ~400
  distinct regions down to 5, or up to several hundred more, gets flagged,
  along with a preview of the specific values that appeared or disappeared.

By default `drift check` exits `1` when drift is found (suitable for CI).
Pass `--mode warn` to only print the findings (prefixed `WARNING:`) and
always exit `0`.

## Transform

`csvsum transform` uses [Polars](https://pola.rs) (lazy CSV scan → transform →
collect/write) to turn a CSV into a new CSV. There are two ways to drive it:

```sh
# Built-in, hard-coded pipeline for the wine-ratings dataset:
# drop `grape`, cast `rating` float -> integer, keep rating >= 90.
cargo run -- transform default examples/wine-ratings.csv
cargo run -- transform default examples/wine-ratings.csv --output out.csv

# Output to Parquet format (automatically detected by .parquet extension):
cargo run -- transform default examples/wine-ratings.csv --output out.parquet

# Or explicitly set the format using --output-format / --format (-f):
cargo run -- transform default examples/wine-ratings.csv --format parquet
# (writes to examples/wine-ratings.transformed.parquet by default)

# Declarative pipeline driven by a YAML spec:
cargo run -- transform apply examples/wine-ratings.csv --spec my-spec.yaml
cargo run -- transform apply examples/wine-ratings.csv --spec my-spec.yaml --output out.parquet

# Scaffold a YAML spec documenting every available step, so you don't have
# to remember the exact shape of each one:
cargo run -- transform scaffold                      # prints to stdout
cargo run -- transform scaffold --output my-spec.yaml # writes to a file
```

A spec is a list of `steps`, applied top to bottom. Each step is a map with a
`step:` field naming its type:

```yaml
description: "Example: drop grape, cast rating to integer, keep 90 and above"
steps:
  - step: drop_columns
    columns: [grape]
  - step: cast
    column: rating
    to: integer          # integer, float, string, boolean
  - step: filter
    column: rating
    op: gte               # gt, gte, lt, lte, eq, neq
    value: 90
```

Other available steps: `select_columns` (keep only the named columns, in
order) and `rename_column` (`from`/`to`). Both `transform default` and
`transform apply` accept `--delimiter`, `--no-header`, `--output`, and
`--output-format` / `--format` (`csv` or `parquet`). Output format defaults to
`parquet` if the output path ends in `.parquet` or `.pq`, and `csv` otherwise.
When omitted, output defaults to `<name>.transformed.<ext>` next to the input
file. Both print a summary of rows in/out.

## Check

`csvsum check` verifies the integrity and structure of data files (currently Parquet files). It automatically infers the format from the file extension (`.parquet` or `.pq`), or can be specified explicitly with `--format parquet`.

```sh
# Verify a Parquet file (format inferred from .parquet or .pq):
cargo run -- check data.parquet
cargo run -- check data.pq

# Explicitly specify format:
cargo run -- check data.bin --format parquet
```

The verification checks:
- **File size and structure**: verifies non-empty file size (minimum valid Parquet size).
- **Magic bytes**: ensures standard `PAR1` magic number in both the header and footer.
- **Readability and decompression**: reads and decodes the dataset across all row groups via Polars to ensure data pages and dictionary encodings are intact.
- **Schema and null analysis**: reports total rows, total columns, column data types, null counts, and null percentages.
- **Anomalies and warnings**: flags 0-row datasets and 100% null columns.

On success, `csvsum check` prints `status: VALID` with schema details and exits with `0`. If any corruption or validation error occurs, it prints `status: INVALID` with the specific error reasons and exits with `1` (suitable for CI/CD pipelines).

## Development

```sh
make fmt        # cargo fmt
make lint       # cargo clippy -D warnings
make test       # cargo test
make check      # fmt-check + lint + test
```
