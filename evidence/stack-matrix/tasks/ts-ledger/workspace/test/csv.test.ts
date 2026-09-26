import { test } from 'node:test';
import assert from 'node:assert/strict';
import { toCsv } from '../src/csv.ts';

test('rows end with CRLF', () => {
  const csv = toCsv({ lines: [{ description: 'Hosting', quantity: 2, unitCents: 1000, vatPercent: 22 }] });
  assert.equal(csv, 'description,quantity,unit,net,vat\r\nHosting,2,10.00,20.00,22%\r\n');
});

test('fields with commas, quotes or newlines are quoted', () => {
  const csv = toCsv({
    lines: [
      { description: 'Setup, one-off', quantity: 1, unitCents: 500, vatPercent: 22 },
      { description: 'The "pro" plan', quantity: 1, unitCents: 500, vatPercent: 22 },
      { description: 'Line one\nline two', quantity: 1, unitCents: 500, vatPercent: 22 },
    ],
  });
  const rows = csv.split('\r\n');
  assert.equal(rows[1], '"Setup, one-off",1,5.00,5.00,22%');
  assert.equal(rows[2], '"The ""pro"" plan",1,5.00,5.00,22%');
  assert.equal(rows[3], '"Line one\nline two",1,5.00,5.00,22%');
});
