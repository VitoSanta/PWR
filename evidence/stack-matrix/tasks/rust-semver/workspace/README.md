# semver-lite

Semantic versions and npm-style version ranges, with no dependencies.

```rust
use semver_lite::{Version, Range, max_satisfying};

let v: Version = "1.4.0-beta.2".parse()?;
let r: Range = "^1.2.0 || >=3.0.0 <3.1.0".parse()?;
r.matches(&v);
max_satisfying(&versions, &r);   // Option<&Version>, the highest match
```

## Version

- `MAJOR.MINOR.PATCH`, each a non-negative integer without leading zeros
  (`0` is fine, `01` is not), optionally `-PRERELEASE` (dot-separated
  identifiers: digits only without leading zeros, or `[0-9A-Za-z-]+`) and
  `+BUILD` (dot-separated `[0-9A-Za-z-]+`). A leading `v` is accepted.
- Ordering and equality follow semver 2.0 precedence: numbers compare
  numerically; a version with a prerelease is lower than the same version
  without; prerelease identifiers compare left to right, numeric ones
  numerically and below alphanumeric ones, which compare as ASCII; a shorter
  list is lower when all before it are equal. Build metadata is ignored for
  both.
- `Display` writes the version back canonically (no `v`, build kept).

## Range

A range is one or more comparator sets joined by `||`; a version matches if
it matches every comparator of at least one set. Inside a set, comparators
are separated by whitespace. Comparators:

| Written | Means |
|---|---|
| `1.2.3`, `=1.2.3` | exactly (build ignored) |
| `>1.2.3` `>=1.2.3` `<1.2.3` `<=1.2.3` | as written |
| `^1.2.3` | `>=1.2.3 <2.0.0` |
| `^0.2.3` | `>=0.2.3 <0.3.0` |
| `^0.0.3` | `>=0.0.3 <0.0.4` |
| `~1.2.3` | `>=1.2.3 <1.3.0` |
| `~1.2` | `>=1.2.0 <1.3.0` |
| `~1` | `>=1.0.0 <2.0.0` |
| `1.2.x`, `1.2.*`, `1.2` | `>=1.2.0 <1.3.0` |
| `1.x`, `1.*`, `1` | `>=1.0.0 <2.0.0` |
| `*`, `x`, or an empty range | any version |
| `1.2.3 - 2.3.4` | `>=1.2.3 <=2.3.4` (full versions on both sides) |

Operators may be followed by spaces (`>= 1.2.3`). Comparators with an
operator (`>`, `>=`, `<`, `<=`, `=`, `^`, `~`) take a full version, except
`~` which also takes `MAJOR.MINOR` or `MAJOR`.

**Prereleases**: a version with a prerelease matches a set only if it
satisfies it *and* one of the set's comparators names a version with the same
`MAJOR.MINOR.PATCH` and a prerelease. So `>=1.2.3-alpha` matches
`1.2.3-beta` but not `1.2.4-beta`, and `*` or `^1.0.0` never match a
prerelease.

Anything else is a parse error: `"...".parse::<Version>()` and
`parse::<Range>()` return `Err(semver_lite::Error)`, which implements
`Display` and `std::error::Error`.

`max_satisfying(versions, range)` is the highest version in the slice that
the range matches, or `None`.

Run the tests with `cargo test`.
