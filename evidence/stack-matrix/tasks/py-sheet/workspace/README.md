# sheet

A small spreadsheet engine in pure Python (standard library only, Python 3.10+).
The acceptance suite in `tests/` is the specification; this file summarises it.

## API

```python
from sheet import Sheet

s = Sheet()
s.set("A1", "10")          # raw input, always a string
s.set("B1", "=A1*2")       # a formula starts with "="
s.get("B1")                # -> 20, the current computed value
s.set("A1", "")            # an empty string clears the cell
Sheet.from_csv(text)       # cells from CSV, row 1 = A1, B1, ...
s.to_csv()                 # computed values over the used rectangle
```

`get` returns `None` for an empty cell, an `int` or `float` for numbers, a `str`
for text, `True`/`False` for comparisons, and an error code string for errors.
Cell references are case-insensitive, columns can have several letters (`AA10`).
Values are always current: changing a cell changes every cell that depends on it.

## Inputs

- A string that parses as an integer is an `int`, as a decimal a `float`.
- Any other string not starting with `=` is text.

## Formulas

- Numbers, cell references, parentheses, and string literals in double quotes.
- Operators, lowest precedence first: comparisons `= <> < > <= >=` (they return
  booleans), `&` (text concatenation, numbers are written as they would print),
  `+ -`, `* /`, `^` (power, right-associative), unary `-` (binds tighter than `^`,
  so `=-2^2` is `4`).
- An empty cell used in a formula counts as `0`.
- A whole-number result may be returned as `int` or `float`; tests compare with `==`.
- Functions, names case-insensitive: `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`
  (numbers only), `IF(condition, then, else)`, `ABS(x)`, `ROUND(x, digits)`.
  Arguments can be values, expressions, nested calls and rectangular ranges
  such as `A1:B3`; in a range, empty and text cells are skipped.

## Errors

A formula's value is an error code string when it cannot be computed:

| Code | When |
|---|---|
| `#DIV/0!` | division by zero |
| `#VALUE!` | text used where a number is needed |
| `#NAME?` | unknown function |
| `#ERROR!` | a formula that does not parse |
| `#CYCLE!` | the cell is on a reference cycle, or depends on one |

Errors propagate: a formula using a cell (or a range containing a cell) whose
value is an error has that error as its value. Fixing the cause fixes every
dependent cell.

## CSV

- `from_csv`: row `n`, column `m` of the CSV goes to the cell in column `m`
  (A, B, ...) and row `n`; empty fields leave the cell empty. Formulas stay live.
- `to_csv`: the rectangle from A1 to the last used row and column, one CSV row
  per sheet row, lines ending in `\n`, standard CSV quoting. Numbers print as
  Python prints them (`1`, `0.25`), booleans as `TRUE`/`FALSE`, errors as their
  code, empty cells as empty fields.

Run the suite with `python3 -m unittest discover -s tests -v`.
