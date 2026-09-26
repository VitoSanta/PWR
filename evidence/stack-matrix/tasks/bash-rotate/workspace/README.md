# rotate

`bin/rotate`, a log rotation script in portable Bash (4+ features are fine,
but it must also run on the Bash 3.2 macOS ships), using only standard tools
(`mv`, `rm`, `gzip`, `chmod`...).

    bin/rotate [--keep N] [--compress] DIR

For every regular file in `DIR` (not in subdirectories) whose name ends in
`.log` and which is not empty, in name order:

1. Numbered copies shift up: `NAME.log.1` becomes `NAME.log.2`, and so on;
   with `--compress` they are `NAME.log.1.gz`, `NAME.log.2.gz`... Copies
   numbered above `N` are deleted (default `N` is 5).
2. `NAME.log` becomes `NAME.log.1` (gzipped to `NAME.log.1.gz` with
   `--compress`).
3. A new empty `NAME.log` is created with the same permission bits as the
   old one.
4. The script prints `rotated NAME.log` on standard output.

Empty `.log` files, symbolic links and anything else are left alone. Names
with spaces work. Options may come in any order before `DIR`.

Errors go to standard error and the exit status is 2: no `DIR`, a `DIR` that
is not a directory, an unknown option, or `--keep` without a whole number
from 1 to 99. Nothing is touched then. Otherwise the exit status is 0.

Run the tests with `python3 -m unittest discover -s tests -v`.
