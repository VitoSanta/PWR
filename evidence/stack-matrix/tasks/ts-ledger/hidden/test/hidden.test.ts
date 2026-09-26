import { test } from 'node:test';
import assert from 'node:assert/strict';
import { roundCents, percentOf } from '../src/money.ts';
import { dueDate } from '../src/dates.ts';
import { toCsv } from '../src/csv.ts';
import { totals, sortedLines } from '../src/invoice.ts';

test('hidden: negative halves round away from zero', () => {
  assert.equal(roundCents(-2.5), -3);
  assert.equal(roundCents(2.5), 3);
});

test('hidden: a due date from a Sunday counts Monday first', () => {
  assert.equal(dueDate('2026-09-27', 1), '2026-09-28');
});

test('hidden: carriage returns are quoted too', () => {
  const csv = toCsv({ lines: [{ description: 'a\rb', quantity: 1, unitCents: 100, vatPercent: 10 }] });
  assert.equal(csv.split('\r\n')[1], '"a\rb",1,1.00,1.00,10%');
});

test('hidden: sorting leaves the invoice untouched', () => {
  const invoice = { lines: [
    { description: 'b', quantity: 1, unitCents: 100, vatPercent: 10 },
    { description: 'a', quantity: 1, unitCents: 100, vatPercent: 10 },
  ] };
  sortedLines(invoice);
  assert.equal(invoice.lines[0].description, 'b');
});

test('hidden: order discount reduces the VAT base', () => {
  const t = totals({ orderDiscountPercent: 50, lines: [{ description: 'x', quantity: 1, unitCents: 10000, vatPercent: 20 }] });
  assert.equal(t.vatByRate['20'], 1000);
  assert.equal(percentOf(10000, 20), 2000);
});
