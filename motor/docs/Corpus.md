# Parser corpus scoreboard

The corpus is a local collection of `.m` files used to measure parser coverage and regressions. Its canonical location is `~/.cache/indymat-motor-fleet/corpus/`. It contains GPL package code, is for testing only, and must never be committed to this repository.

Run the scoreboard from the repository root:

```sh
cargo run -p indy-corpus --release -- ~/.cache/indymat-motor-fleet/corpus/ --dialect octave --json current.json --fail-list failures.txt
```

The report groups results by each file's first directory below the corpus root (files directly in the root appear under `(root)`). It shows successful parses, failures, parser panics, totals, and success percentage, followed by frequent diagnostic codes and slow files. Invalid UTF-8 input is decoded as Latin-1 and counted in the report. `--dialect matlab` selects MATLAB mode.

Compare a new run with a saved result:

```sh
cargo run -p indy-corpus --release -- ~/.cache/indymat-motor-fleet/corpus/ --baseline previous.json --json current.json
```

The regression list names files that passed in the baseline and fail now; these make the command exit with status 1. Newly passing files are also listed. The baseline is the JSON output produced by this binary.

The optional GNU Octave parse oracle checks syntax without executing corpus files:

```sh
scripts/octave-parse-oracle.sh ~/.cache/indymat-motor-fleet/corpus/ octave-results.tsv
```

It starts one `octave-cli --no-gui --norc --quiet` process and calls Octave's `__parse_file__` for each path in a generated driver. TSV rows contain path, `ok` or `fail`, and the error message. Running the oracle requires GNU Octave on `PATH`; the orchestrator should run it where Octave is available.
