# rules

A tiny rule language for feature flags, in Kotlin (JVM), built with Gradle.
Package `rules`.

```kotlin
val rule = Rules.parse("""country in ["IT", "FR"] and (age >= 18 or plan == "pro")""")
Rules.evaluate(rule, mapOf("country" to "IT", "age" to 17, "plan" to "pro"))   // true
```

```kotlin
sealed class Expr { abstract val position: Int }      // your node types extend it
class RuleError(message: String, val position: Int) : Exception(message)

object Rules {
    fun parse(source: String): Expr                            // throws RuleError
    fun evaluate(expr: Expr, context: Map<String, Any?>): Boolean  // throws RuleError
}
```

Positions are 0-based character offsets into the source.

## Syntax

- **Literals**: integers (`18`, `-2`), decimals (`4.5`, `-0.5`), strings in double quotes
  with `\"` and `\\` escapes, `true`, `false`, and lists of literals
  `["IT", "FR"]` (empty allowed).
- **Names**: `[A-Za-z_][A-Za-z0-9_.]*`, looked up in the context by the whole
  name (`user.age` is the key `"user.age"`). `and`, `or`, `not`, `in`,
  `true`, `false` are keywords, not names.
- **Comparisons**: `==`, `!=`, `<`, `<=`, `>`, `>=` between two operands
  (a literal or a name), and `operand in [list]`, `operand not in [list]`.
  The right side of `in` is always a list literal.
- **Logic**: `not`, `and`, `or`, parentheses. Precedence from loosest:
  `or`, `and`, `not`, comparison. A bare name or `true`/`false` is also an
  expression.
- Whitespace between tokens is free.

A syntax error is a `RuleError` at the position of the offending token (or of
the end of the source when it ends too early): an unexpected character, an
unterminated string (at its opening quote), a missing `)`, anything left after
a complete expression, a missing operand.

## Meaning

- Numbers compare as numbers, integers and decimals alike (`18 == 18.0`).
  Strings compare as strings (`<` and friends lexicographically). Booleans
  only with `==` and `!=`. Comparing a string with a number, or ordering
  booleans, is a `RuleError` at the position where the comparison starts
  (its left operand).
- A name missing from the context (or mapped to `null`) makes every
  comparison it takes part in false -- `==`, `!=` and `in` alike -- so
  `not (age > 18)` is true when there is no `age`.
- `x in [...]` is true when `x` equals an element by the rules above
  (elements of another type simply do not match).
- A bare name must hold a Boolean; anything else (a missing name included)
  is a `RuleError` at its position.

Run the tests with `gradle test`.
