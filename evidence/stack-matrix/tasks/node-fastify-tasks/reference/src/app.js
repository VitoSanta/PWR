import Fastify from 'fastify';

const STATUSES = ['todo', 'doing', 'done'];
const MOVES = { todo: ['doing'], doing: ['done', 'todo'], done: ['todo'] };
const FIELDS = ['title', 'status', 'priority', 'due', 'labels'];

function realDate(text) {
  if (typeof text !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(text)) return false;
  const [y, m, d] = text.split('-').map(Number);
  const date = new Date(Date.UTC(y, m - 1, d));
  return date.getUTCFullYear() === y && date.getUTCMonth() === m - 1 && date.getUTCDate() === d;
}

function validate(body, { partial }) {
  const details = [];
  if (body === null || typeof body !== 'object' || Array.isArray(body)) {
    return { details: ['body must be an object'] };
  }
  for (const key of Object.keys(body)) {
    if (!FIELDS.includes(key)) details.push(`unknown property ${key}`);
  }
  const clean = {};
  if ('title' in body || !partial) {
    const title = typeof body.title === 'string' ? body.title.trim() : '';
    if (title.length < 1 || title.length > 200) details.push('title must be 1-200 characters');
    else clean.title = title;
  }
  if ('status' in body) {
    if (!STATUSES.includes(body.status)) details.push('status must be todo, doing or done');
    else clean.status = body.status;
  }
  if ('priority' in body) {
    if (!Number.isInteger(body.priority) || body.priority < 1 || body.priority > 4) details.push('priority must be an integer from 1 to 4');
    else clean.priority = body.priority;
  }
  if ('due' in body) {
    if (body.due !== null && !realDate(body.due)) details.push('due must be a real YYYY-MM-DD date or null');
    else clean.due = body.due;
  }
  if ('labels' in body) {
    if (!Array.isArray(body.labels) || body.labels.some((l) => typeof l !== 'string')) details.push('labels must be strings');
    else clean.labels = [...new Set(body.labels.map((l) => l.toLowerCase()))];
  }
  return { details, clean };
}

const today = () => new Date().toISOString().slice(0, 10);
const overdue = (task, day) => task.due !== null && task.due < day && task.status !== 'done';

export function buildApp() {
  const app = Fastify();
  const tasks = new Map();
  let next = 1;

  const invalid = (reply, details) => reply.code(400).send({ error: 'validation', details });
  const find = (request, reply) => {
    const task = tasks.get(Number(request.params.id));
    if (!task) reply.code(404).send({ error: 'not found' });
    return task;
  };

  app.post('/tasks', async (request, reply) => {
    const { details, clean } = validate(request.body, { partial: false });
    if (details.length) return invalid(reply, details);
    const task = { id: next++, title: clean.title, status: clean.status ?? 'todo', priority: clean.priority ?? 3,
      due: clean.due ?? null, labels: clean.labels ?? [], version: 1 };
    tasks.set(task.id, task);
    return reply.code(201).header('location', `/tasks/${task.id}`).send(task);
  });

  app.get('/tasks', async (request) => {
    const { status, label, overdue: onlyOverdue } = request.query;
    const day = request.query.today ?? today();
    return [...tasks.values()]
      .filter((t) => !status || t.status === status)
      .filter((t) => !label || t.labels.includes(label.toLowerCase()))
      .filter((t) => onlyOverdue !== 'true' || overdue(t, day))
      .sort((a, b) => a.priority - b.priority
        || (a.due === b.due ? 0 : a.due === null ? 1 : b.due === null ? -1 : a.due < b.due ? -1 : 1)
        || a.id - b.id);
  });

  app.get('/tasks/:id', async (request, reply) => find(request, reply));

  app.patch('/tasks/:id', async (request, reply) => {
    const task = find(request, reply);
    if (!task) return reply;
    const match = request.headers['if-match'];
    if (match === undefined) return reply.code(428).send({ error: 'precondition required' });
    const { details, clean } = validate(request.body ?? {}, { partial: true });
    if (details.length) return invalid(reply, details);
    if (Number(match) !== task.version) return reply.code(409).send({ error: 'conflict', current: task });
    Object.assign(task, clean, { version: task.version + 1 });
    return task;
  });

  app.post('/tasks/:id/transition', async (request, reply) => {
    const task = find(request, reply);
    if (!task) return reply;
    const to = request.body?.to;
    if (!STATUSES.includes(to)) return invalid(reply, ['to must be todo, doing or done']);
    if (!MOVES[task.status].includes(to)) {
      return reply.code(422).send({ error: 'invalid transition', from: task.status, to });
    }
    task.status = to;
    task.version += 1;
    return task;
  });

  app.delete('/tasks/:id', async (request, reply) => {
    const task = find(request, reply);
    if (!task) return reply;
    tasks.delete(task.id);
    return reply.code(204).send();
  });

  app.get('/stats', async (request) => {
    const day = request.query.today ?? today();
    const all = [...tasks.values()];
    const byStatus = Object.fromEntries(STATUSES.map((s) => [s, all.filter((t) => t.status === s).length]));
    return { total: all.length, byStatus, overdue: all.filter((t) => overdue(t, day)).length };
  });

  return app;
}
