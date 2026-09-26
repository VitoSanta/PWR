# duration

Reads and writes durations such as `1h30m` or `1w2d3h4m5s6ms`, in Zig 0.16.0.
The library is the module `duration`, in `src/duration.zig`. `build.zig` runs
every file in `tests/` against it with `zig build test`; the tests are the
contract. The code is pure: no allocation, no I/O.

## API

```zig
pub const ParseError = error{ Empty, MissingNumber, MissingUnit, UnknownUnit, UnitOrder, Overflow };

/// The duration `text` names, in milliseconds.
pub fn parse(text: []const u8) ParseError!u64;

/// Writes the canonical form of `ms` into `buf` and returns the part written.
pub fn format(ms: u64, buf: []u8) error{NoSpaceLeft}![]const u8;
```

## Units

| Unit | Milliseconds |
|---|---|
| `w` | 604 800 000 (7 days) |
| `d` | 86 400 000 |
| `h` | 3 600 000 |
| `m` | 60 000 |
| `s` | 1 000 |
| `ms` | 1 |

Units are lowercase only.

## parse

The text is one or more segments written together, each `<digits><unit>`:
no spaces, no sign, no decimal point. Leading zeros are allowed, however
many (`007m` is 7 minutes). A segment's unit is the longest run of ASCII
letters after its digits, and it must be exactly one of the units above.
Units must appear in strictly decreasing size: each segment's unit is smaller
than the one before it, so a unit never repeats. A segment may be zero
(`0h0m` is 0).

Segments are read left to right, and the first problem found is the error
returned. Within one segment the checks come in this order:

1. `error.MissingNumber` -- the segment does not start with a digit
   (`h`, `-5s`, the space in `1h 30m`, the `!` in `5s!`).
2. `error.MissingUnit` -- digits with no letters after them (`10`, `1h30`).
3. `error.UnknownUnit` -- letters that are not a unit (`5y`, `5sec`, `1H`, `1hh`).
4. `error.UnitOrder` -- a unit not smaller than the previous segment's
   (`30m1h`, `1h1h`, `1ms5s`).
5. `error.Overflow` -- the segment's value, or the total so far, does not fit
   in a `u64` number of milliseconds.

The empty text is `error.Empty`.

So `99999999999999999999` is `MissingUnit` and `99999999999999999999ms` is
`Overflow`: a number too large for a `u64` is only an overflow once the
segment is otherwise well formed.

## format

The canonical form: from weeks down to milliseconds, each component written
only when it is not zero, as `<number><unit>` with nothing between them.
Zero is `0ms`. For example 5 400 000 is `1h30m`, 604 800 001 is `1w1ms`, and
90 061 001 is `1d1h1m1s1ms`.

`format` returns `error.NoSpaceLeft` when `buf` is too small for the whole
form. For every `u64` value `v`, `parse(format(v)) == v`.
