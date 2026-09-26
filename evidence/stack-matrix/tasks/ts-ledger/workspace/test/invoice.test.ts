import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sortedLines, totals, type Invoice } from '../src/invoice.ts';

test('a plain line: VAT on the net', () => {
  const t = totals({ lines: [{ description: 'Hosting', quantity: 2, unitCents: 1000, vatPercent: 22 }] });
  assert.deepEqual(t, { net: 2000, discount: 0, vat: 440, gross: 2440, vatByRate: { '22': 440 } });
});

test('a line discount comes before VAT', () => {
  const t = totals({
    lines: [{ description: 'Seats', quantity: 3, unitCents: 999, discountPercent: 10, vatPercent: 22 }],
  });
  assert.deepEqual(t, { net: 2997, discount: 300, vat: 593, gross: 3290, vatByRate: { '22': 593 } });
});

test('the order discount also comes before VAT', () => {
  const t = totals({
    lines: [{ description: 'Licence', quantity: 1, unitCents: 10000, vatPercent: 22 }],
    orderDiscountPercent: 10,
  });
  assert.deepEqual(t, { net: 10000, discount: 1000, vat: 1980, gross: 10980, vatByRate: { '22': 1980 } });
});

test('VAT is computed per rate', () => {
  const t = totals({
    lines: [
      { description: 'A', quantity: 1, unitCents: 5000, vatPercent: 22 },
      { description: 'B', quantity: 2, unitCents: 1000, vatPercent: 4 },
      { description: 'C', quantity: 1, unitCents: 3000, discountPercent: 50, vatPercent: 22 },
    ],
  });
  assert.deepEqual(t, { net: 10000, discount: 1500, vat: 1510, gross: 10010, vatByRate: { '22': 1430, '4': 80 } });
});

test('printing order does not reorder the invoice it was given', () => {
  const invoice: Invoice = {
    lines: [
      { description: 'b', quantity: 1, unitCents: 100, vatPercent: 22 },
      { description: 'a', quantity: 1, unitCents: 100, vatPercent: 22 },
    ],
  };
  const sorted = sortedLines(invoice);
  assert.deepEqual(sorted.map((line) => line.description), ['a', 'b']);
  assert.deepEqual(invoice.lines.map((line) => line.description), ['b', 'a']);
});
