import { TestBed } from '@angular/core/testing';
import { AgentStore } from './agent.store';
import { Entry } from './model';
import { RunStore } from './run';

let at = 0;
const entry = (kind: Entry['kind'], fields: Partial<Entry> = {}): Entry =>
  ({ key: `e${++at}`, kind, title: '', text: '', status: 'done', at, ...fields }) as Entry;
const tool = (toolKind: string, title: string, text = '') => entry('tool', { toolKind, title, text } as Partial<Entry>);

describe('RunStore', () => {
  let store: AgentStore;
  let run: RunStore;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    store = TestBed.inject(AgentStore);
    run = TestBed.inject(RunStore);
  });

  it('reads only the turn after the last message', () => {
    store.timeline.set([
      entry('user', { text: 'first' }),
      tool('edit', 'apply_replace a.ts'),
      entry('user', { text: 'second' }),
      tool('read', 'read_file b.ts'),
    ]);
    expect(run.prompt()?.text).toBe('second');
    expect(run.actions().map((action) => action.title)).toEqual(['read_file b.ts']);
  });

  it('lays the run over the loop: reached stages done, the current one active', () => {
    store.timeline.set([
      entry('user', { text: 'fix it' }),
      entry('thought', { text: 'plan' }),
      tool('read', 'read_file a.ts'),
      tool('edit', 'apply_replace a.ts'),
    ]);
    store.turnActive.set(true);
    const stages = Object.fromEntries(run.stages().map((stage) => [stage.short, stage.state]));
    expect(stages).toEqual({ Plan: 'done', Inspect: 'done', Implement: 'active', Verify: 'waiting' });
    expect(run.state()).toBe('working');
  });

  it('shows Fix only once the run has had to fix something', () => {
    store.timeline.set([
      entry('user', { text: 'fix it' }),
      tool('edit', 'apply_replace a.ts'),
      tool('execute', 'run_command', 'npm test'),
      tool('edit', 'apply_replace a.ts'),
    ]);
    expect(run.stages().map((stage) => stage.short)).toEqual(['Plan', 'Inspect', 'Implement', 'Verify', 'Fix']);
    const verify = run.stages().find((stage) => stage.short === 'Verify')!;
    expect(verify.summary?.checks).toBe(1);
  });

  it('is asking while a permission waits, whatever else runs', () => {
    store.turnActive.set(true);
    store.permission.set({ id: 1, title: 'git push', approval: 'publish', options: [] });
    expect(run.state()).toBe('asking');
  });
});
