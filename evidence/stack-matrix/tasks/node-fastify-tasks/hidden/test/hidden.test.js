import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildApp } from '../src/app.js';

const post = (app, payload) => app.inject({ method: 'POST', url: '/tasks', payload });

test('hidden: leap days are real dates, and null clears a due date', async () => {
  const app = buildApp();
  assert.equal((await post(app, { title: 'x', due: '2028-02-29' })).statusCode, 201);
  assert.equal((await post(app, { title: 'x', due: '2027-02-29' })).statusCode, 400);
  const response = await app.inject({ method: 'PATCH', url: '/tasks/1', headers: { 'if-match': '1' }, payload: { due: null } });
  assert.equal(response.statusCode, 200);
  assert.equal(response.json().due, null);
});

test('hidden: tasks without a due date sort after those with one', async () => {
  const app = buildApp();
  await post(app, { title: 'none', priority: 1 });
  await post(app, { title: 'late', priority: 1, due: '2030-01-01' });
  await post(app, { title: 'soon', priority: 1, due: '2026-01-01' });
  assert.deepEqual((await app.inject('/tasks')).json().map((t) => t.title), ['soon', 'late', 'none']);
});

test('hidden: done tasks are never overdue', async () => {
  const app = buildApp();
  await post(app, { title: 'a', due: '2020-01-01', status: 'done' });
  await post(app, { title: 'b', due: '2020-01-01', status: 'doing' });
  assert.deepEqual((await app.inject('/tasks?overdue=true&today=2021-01-01')).json().map((t) => t.title), ['b']);
  assert.equal((await app.inject('/stats?today=2021-01-01')).json().overdue, 1);
});

test('hidden: a patch with unknown fields or a bad if-match is refused', async () => {
  const app = buildApp();
  await post(app, { title: 'a' });
  const unknown = await app.inject({ method: 'PATCH', url: '/tasks/1', headers: { 'if-match': '1' }, payload: { owner: 'me' } });
  assert.equal(unknown.statusCode, 400);
  const absent = await app.inject({ method: 'PATCH', url: '/tasks/9', headers: { 'if-match': '1' }, payload: { title: 'x' } });
  assert.equal(absent.statusCode, 404);
});

test('hidden: transition of an unknown task or to an unknown status', async () => {
  const app = buildApp();
  assert.equal((await app.inject({ method: 'POST', url: '/tasks/5/transition', payload: { to: 'doing' } })).statusCode, 404);
  await post(app, { title: 'a' });
  assert.equal((await app.inject({ method: 'POST', url: '/tasks/1/transition', payload: { to: 'archived' } })).statusCode, 400);
});
