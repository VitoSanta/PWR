# cron

Cron expressions in Dart: parse one, test a time against it, find the next
time it fires. `lib/cron.dart` exports:

```dart
class CronExpression {
  static CronExpression parse(String source);   // throws FormatException
  bool matches(DateTime time);                  // compared in UTC, to the minute
  DateTime next(DateTime after);                // the first matching minute strictly after `after`, in UTC
}
```

## Syntax

Five fields separated by whitespace: minute (0-59), hour (0-23), day of
month (1-31), month (1-12 or `JAN`-`DEC`), day of week (0-7 or `SUN`-`SAT`,
both 0 and 7 being Sunday). Names are case-insensitive.

Each field is a comma-separated list of: `*`, a value, a range `a-b`
(`a <= b`), or any of those with a step `/n` (`*/15`, `10-40/10`; `n >= 1`).
A step on a single value `a/n` means `a-max/n`.

Macros: `@yearly` (`0 0 1 1 *`), `@monthly` (`0 0 1 * *`), `@weekly`
(`0 0 * * 0`), `@daily` (`0 0 * * *`), `@hourly` (`0 * * * *`).

Anything else -- the wrong number of fields, a value out of range, a
reversed range, a step of 0, an unknown name -- throws a `FormatException`.

## Matching

Minute, hour and month must match. For the days, as classic cron does: if
both day of month and day of week are restricted (neither is `*`), a day
matches when **either** matches; otherwise both must. (`*/1` counts as
restricted; only a bare `*` does not.)

`next` works in UTC (a non-UTC `after` is converted first), drops seconds and
smaller, and returns the first matching minute after `after`. An expression
that can never fire (`0 0 30 2 *`) makes `next` throw a `StateError`.

Run the tests with `dart test` after `dart pub get`.
