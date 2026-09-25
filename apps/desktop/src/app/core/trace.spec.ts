import { Entry } from './model';
import { compactTurn, groupSteps, runMetrics, runOutcome, toolCategory } from './trace';

let clock = 0;
const entry = (kind: Entry['kind'], extra: Partial<Entry> = {}): Entry => ({
  key: `k${++clock}`,
  kind,
  title: kind,
  text: '',
  status: 'done',
  at: clock * 1000,
  ...extra,
});
const tool = (toolKind: string, text: string, extra: Partial<Entry> = {}) =>
  entry('tool', { toolKind, text, title: `${toolKind === 'execute' ? 'run_command' : 'read_file ' + text}`, ...extra });

describe('the execution trace', () => {
  beforeEach(() => (clock = 0));

  it('names phases by what the work did', () => {
    const entries = [
      entry('thought', { text: 'Look first.' }),
      tool('read', 'src/a.ts'),
      tool('search', '"router"'),
      tool('edit', 'src/a.ts', { title: 'apply_replace src/a.ts' }),
      tool('execute', 'npm test'),
      tool('edit', 'src/a.ts', { title: 'apply_replace src/a.ts' }),
      entry('reply', { text: 'Done.' }),
    ];
    const turn = compactTurn(entries, false);
    expect(turn.phases.map((phase) => phase.name)).toEqual(['Planning', 'Inspecting workspace', 'Implementing', 'Verifying', 'Fixing']);
    expect(turn.result?.text).toBe('Done.');
  });

  it('never takes text written before a retry as the result', () => {
    const entries = [tool('read', 'a'), entry('reply', { text: '{"broken' }), entry('retry', { data: { cause: 'reply_fault' } })];
    const turn = compactTurn(entries, true);
    expect(turn.result).toBeNull();
    expect(turn.phases[0].entries.map((item) => item.kind)).toEqual(['tool', 'reply', 'retry']);
  });

  it('adds Finalizing while the answer streams', () => {
    const turn = compactTurn([tool('read', 'a'), entry('reply', { text: 'Wri', status: 'live' })], true);
    expect(turn.phases.at(-1)?.name).toBe('Finalizing');
  });

  it('tells a check from another command, and a malformed call from a refusal', () => {
    expect(toolCategory(tool('execute', 'cargo test -p pwr-cli'))).toBe('verification');
    expect(toolCategory(tool('execute', 'npm install'))).toBe('command');
    expect(toolCategory(tool('other', 'missing field `path`', { status: 'failed' }))).toBe('malformed');
    expect(toolCategory(tool('edit', 'outside the workspace', { status: 'failed' }))).toBe('refused');
  });

  it('keeps retries inside the action group they interrupt, and generations out of Detailed', () => {
    const steps = groupSteps([tool('read', 'a'), entry('retry'), entry('generation'), tool('read', 'b')]);
    expect(steps.length).toBe(1);
    expect(steps[0].type === 'actions' && steps[0].entries.map((item) => item.kind)).toEqual(['tool', 'retry', 'tool']);
  });

  it('offers Retry or Continue only where the core gave up or paused', () => {
    expect(runOutcome({ _meta: { pwr: { terminal: 'protocol', actions: 3, stoppedBecause: 'x' } } }, false)).toMatchObject({ action: 'retry', detail: 'x' });
    expect(runOutcome({ _meta: { pwr: { terminal: 'budget', actions: 26 } } }, false).action).toBe('continue');
    expect(runOutcome({ _meta: { pwr: { terminal: 'recovery' } } }, false).action).toBeNull();
    expect(runOutcome({ stopReason: 'end_turn', _meta: { pwr: { actions: 1 } } }, false)).toMatchObject({ action: null, text: 'Finished after 1 action.' });
  });

  it('reads runtime figures only from what was reported', () => {
    const entries = [
      entry('user'),
      entry('generation', { data: { promptTokens: 1000, generatedTokens: 200, promptEvalMs: 500, generationMs: 4000, firstChunkMs: 600 } }),
      tool('read', 'a.ts', { data: { path: 'a.ts' } }),
      tool('read', 'a.ts', { data: { path: 'a.ts' } }),
      tool('execute', 'npm test'),
      entry('retry'),
      entry('recovery', { data: { retries: 1 } }),
      entry('generation', { data: { promptTokens: 1300, generatedTokens: 100, promptEvalMs: null, generationMs: null } }),
    ];
    const metrics = runMetrics(entries, 0, false);
    expect(metrics.generationTps).toBeNull();
    expect(metrics.promptEvalTps).toBeNull();
    expect(metrics.runInputTokens).toBe(2300);
    expect(metrics.runOutputTokens).toBe(300);
    expect(metrics.filesRead).toBe(1);
    expect(metrics).toMatchObject({ toolCalls: 3, commands: 1, verifications: 1, retries: 1, recovered: 1, generations: 2 });
    expect(runMetrics(entries.slice(0, 2), 0, false).generationTps).toBe(50);
  });
});
