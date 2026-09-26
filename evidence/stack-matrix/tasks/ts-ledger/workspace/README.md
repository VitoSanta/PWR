# ledger

Billing core used by the invoicing service: money arithmetic in integer
cents, line and order discounts, VAT, due dates in business days, and a CSV
export for accounting.

Rules the finance team signed off (the tests encode them):

- Money is integer cents. Rounding is half away from zero: 0.5 cent rounds to
  1 cent and -0.5 cent to -1 cent.
- A line's discount applies to that line's net amount. The order discount
  applies to the sum of the discounted lines. Discounts come before VAT.
- VAT is computed per rate on the discounted net, rounded once per rate.
- A due date counts business days (Monday to Friday) after the issue date;
  the issue date itself never counts, whatever day it is.
- Functions never modify the objects or arrays they are given.
- The CSV export follows RFC 4180: fields with a comma, a quote or a newline
  are quoted, quotes doubled; lines end with CRLF.

Run the tests with `npm test` (Node 22.6 or later runs the TypeScript directly).
