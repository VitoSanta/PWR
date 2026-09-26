# tips

A tip calculator, as a Flutter widget: `TipCalculator` in
`lib/tip_calculator.dart` (a `StatefulWidget` with a `const` constructor),
built with Material widgets.

## What it shows

- A `TextField` labelled `Bill` (its `InputDecoration.labelText`) for the
  bill amount, a decimal number such as `42.50`. Anything that is not a
  number of zero or more -- text, a negative number -- shows the field's
  `errorText` `Enter a valid amount`; an empty field shows no error.
- Three `ChoiceChip`s, `10%`, `15%`, `20%`; `15%` is selected at first, and
  selecting one deselects the others.
- The number of people splitting the bill, shown as `People: N` (1 at
  first), with two `IconButton`s whose tooltips are `Remove person` and
  `Add person`; N stays between 1 and 20 (the buttons are disabled at the
  ends -- `onPressed: null`).
- Three `Text`s: `Tip: $T`, `Total: $S`, `Per person: $P`, each amount with
  two decimals. When the bill is empty or invalid all three amounts are `—`
  (an em dash): `Tip: —`.

## The arithmetic

In cents: tip = the bill times the percentage, rounded half up to the cent;
total = bill + tip; per person = total divided by the people, rounded **up**
to the cent (so the shares always cover the total).

Run the tests with `flutter test`.
