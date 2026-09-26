import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildApp } from '../src/app.js';

async function create(app, body) {
  const response = await app.inject({ method: 'POST', url: '/tasks', payload: body });
  assert.equal(response.statusCode, 201, response.body);
  return response.json();
}

test('creates with defaults and a location', async () => {
  const app = buildApp();
  const response = await app.inject({ method: 'POST', url: '/tasks', payload: { title: '  Write docs ', labels: ['Docs', 'docs', 'API'] } });
  assert.equal(response.statusCode, 201);
  assert.equal(response.headers.location, '/tasks/1');
  assert.deepEqual(response.json(), { id: 1, title: 'Write docs', status: 'todo', priority: 3, due: null, labels: ['docs', 'api'], version: 1 });
});

test('validates the body', async () => {
  const app = buildApp();
  for (const payload of [{}, { title: '' }, { title: 'x', priority: 5 }, { title: 'x', due: '2026-02-30' }, { title: 'x', status: 'blocked' }, { title: 'x', extra: 1 }, { title: 'x'.repeat(201) }]) {
    const response = await app.inject({ method: 'POST', url: '/tasks', payload });
    assert.equal(response.statusCode, 400, JSON.stringify(payload));
    const body = response.json();
    assert.equal(body.error, 'validation');
    assert.ok(Array.isArray(body.details) && body.details.length > 0);
  }
});

test('lists by priority, due date, then id, with filters', async () => {
  const app = buildApp();
  await create(app, { title: 'a', priority: 2 });
  await create(app, { title: 'b', priority: 1, due: '2026-10-05' });
  await create(app, { title: 'c', priority: 2, due: '2026-09-01', labels: ['ops'] });
  await create(app, { title: 'd', priority: 1, due: '2026-10-01', status: 'done' });
  const all = (await app.inject('/tasks')).json();
  assert.deepEqual(all.map((t) => t.title), ['d', 'b', 'c', 'a']);
  const todo = (await app.inject('/tasks?status=todo')).json();
  assert.deepEqual(todo.map((t) => t.title), ['b', 'c', 'a']);
  const ops = (await app.inject('/tasks?label=OPS')).json();
  assert.deepEqual(ops.map((t) => t.title), ['c']);
  const overdue = (await app.inject('/tasks?overdue=true&today=2026-10-02')).json();
  assert.deepEqual(overdue.map((t) => t.title), ['c']);
});

test('patch needs the current version', async () => {
  const app = buildApp();
  const task = await create(app, { title: 'a' });
  const missing = await app.inject({ method: 'PATCH', url: `/tasks/${task.id}`, payload: { priority: 1 } });
  assert.equal(missing.statusCode, 428);
  const ok = await app.inject({ method: 'PATCH', url: `/tasks/${task.id}`, headers: { 'if-match': '1' }, payload: { priority: 1, labels: ['X'] } });
  assert.equal(ok.statusCode, 200);
  assert.deepEqual(ok.json(), { ...task, priority: 1, labels: ['x'], version: 2 });
  const stale = await app.inject({ method: 'PATCH', url: `/tasks/${task.id}`, headers: { 'if-match': '1' }, payload: { title: 'b' } });
  assert.equal(stale.statusCode, 409);
  assert.equal(stale.json().error, 'conflict');
  assert.equal(stale.json().current.version, 2);
  const invalid = await app.inject({ method: 'PATCH', url: `/tasks/${task.id}`, headers: { 'if-match': '2' }, payload: { due: 'soon' } });
  assert.equal(invalid.statusCode, 400);
});

test('status moves only along allowed transitions', async () => {
  const app = buildApp();
  const task = await create(app, { title: 'a' });
  const move = (to) => app.inject({ method: 'POST', url: `/tasks/${task.id}/transition`, payload: { to } });
  assert.equal((await move('done')).statusCode, 422);
  assert.deepEqual((await move('done')).json(), { error: 'invalid transition', from: 'todo', to: 'done' });
  const doing = await move('doing');
  assert.equal(doing.statusCode, 200);
  assert.equal(doing.json().status, 'doing');
  assert.equal(doing.json().version, 2);
  assert.equal((await move('done')).json().status, 'done');
  assert.equal((await move('doing')).statusCode, 422);
  assert.equal((await move('todo')).json().status, 'todo');
});

test('404, delete and stats', async () => {
  const app = buildApp();
  assert.deepEqual((await app.inject('/tasks/9')).json(), { error: 'not found' });
  assert.equal((await app.inject('/tasks/9')).statusCode, 404);
  await create(app, { title: 'a', due: '2026-01-01' });
  const b = await create(app, { title: 'b' });
  await create(app, { title: 'c', status: 'done', due: '2026-01-01' });
  assert.equal((await app.inject({ method: 'DELETE', url: `/tasks/${b.id}` })).statusCode, 204);
  assert.equal((await app.inject({ method: 'DELETE', url: `/tasks/${b.id}` })).statusCode, 404);
  assert.equal((await create(app, { title: 'd' })).id, 4);
  assert.deepEqual((await app.inject('/stats?today=2026-06-01')).json(), { total: 3, byStatus: { todo: 2, doing: 0, done: 1 }, overdue: 1 });
});

test('apps do not share a store', async () => {
  const one = buildApp();
  await create(one, { title: 'a' });
  assert.deepEqual((await buildApp().inject('/tasks')).json(), []);
});
