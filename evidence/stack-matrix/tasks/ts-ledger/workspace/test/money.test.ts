import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatMoney, parseMoney, percentOf, roundCents } from '../src/money.ts';

test('rounds half away from zero, both signs', () => {
  assert.equal(roundCents(0.5), 1);
  assert.equal(roundCents(1.5), 2);
  assert.equal(roundCents(2.4), 2);
  assert.equal(roundCents(-0.5), -1);
  assert.equal(roundCents(-1.5), -2);
  assert.equal(roundCents(-2.4), -2);
});

test('parses and formats amounts', () => {
  assert.equal(parseMoney('12.34'), 1234);
  assert.equal(parseMoney('12.3'), 1230);
  assert.equal(parseMoney('-0.5'), -50);
  assert.equal(formatMoney(123450), '1234.50');
  assert.equal(formatMoney(-50), '-0.50');
  assert.throws(() => parseMoney('12,34'));
});

test('a percentage of an amount is rounded once, symmetrically', () => {
  assert.equal(percentOf(1005, 10), 101);
  assert.equal(percentOf(-1005, 10), -101);
  assert.equal(percentOf(2997, 10), 300);
});
