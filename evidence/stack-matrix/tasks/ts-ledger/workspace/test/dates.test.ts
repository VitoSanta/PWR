import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dueDate } from '../src/dates.ts';

test('business days after a weekday', () => {
  assert.equal(dueDate('2026-09-21', 3), '2026-09-24'); // Monday -> Thursday
  assert.equal(dueDate('2026-09-25', 1), '2026-09-28'); // Friday -> Monday
  assert.equal(dueDate('2026-09-24', 10), '2026-10-08'); // Thursday + two weeks
});

test('an invoice issued on a weekend starts counting on Monday', () => {
  assert.equal(dueDate('2026-09-26', 1), '2026-09-28'); // Saturday -> Monday
  assert.equal(dueDate('2026-09-27', 5), '2026-10-02'); // Sunday -> Friday
});
