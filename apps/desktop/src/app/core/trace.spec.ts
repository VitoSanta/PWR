import { Entry } from './model';
import { actionPhrase, compactTurn, groupSteps, runMetrics, runOutcome, toolCategory } from './trace';

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
  it('keeps a goal without an owner acceptance contract paused', () => {
    const outcome = runOutcome({ _meta: { pwr: { goal: { enabled: true, needsAcceptance: true }, outcome: { checks: { status: 'passed' }, acceptance: { status: 'not_declared' } } } } }, false);
    expect(outcome.tone).toBe('paused');
    expect(outcome.text).toContain('no acceptance contract');
  });
  it('shows zero-test evidence even when a legacy goal requests acceptance', () => {
    const outcome = runOutcome({ _meta: { pwr: { goal: { enabled: true, needsAcceptance: true }, outcome: { checks: { status: 'ran_zero_tests' }, acceptance: { status: 'not_declared' } } } } }, false);
    expect(outcome.tone).toBe('paused');
    expect(outcome.text).toContain('zero tests');
  });

  it('keeps retry and continuation available when the terminal cause also has failed checks', () => {
    for (const terminal of ['protocol', 'provider', 'budget']) {
      const outcome = runOutcome({ _meta: { pwr: { terminal, outcome: { checks: { status: 'failed' }, acceptance: { status: 'not_declared' } } } } }, false);
      expect(outcome.action).toBe(terminal === 'budget' ? 'continue' : 'retry');
    }
  });

  it('does not infer work delivery from unavailable checks', () => {
    for (const delivered of [false, true, undefined]) {
      const result = runOutcome({ _meta: { pwr: { outcome: { delivered, checks: { status: 'unavailable', why: 'no checks' } } } } }, false);
      expect(result.text).not.toContain('Work delivered');
      expect(result.text).toContain('unavailable');
    }
  });

  it('uses typed verification evidence and exposes unconfined execution', () => {
    const reply = { _meta: { pwr: { goal: { verified: true }, outcome: { checks: { status: 'ran_zero_tests' }, acceptance: { status: 'not_declared' }, confinement: { status: 'unconfined' } } } } };
    expect(runOutcome(reply, false)).toMatchObject({ tone: 'paused', action: null, confinement: 'Unconfined: commands run with your full rights' });
    expect(runOutcome(reply, false).text).toContain('zero tests');
  });

  it('never labels changed acceptance evidence as verified', () => {
    expect(runOutcome({ _meta: { pwr: { terminal: 'contract_changed', goal: { verified: true, contractChanged: ['tests/acceptance.rs'] } } } }, false)).toMatchObject({ tone: 'failed', detail: 'tests/acceptance.rs', action: null });
  });
  it('shows which goal budget stopped the run', () => {
    const outcome = runOutcome({ _meta: { pwr: { terminal: 'budget', totalActions: 5, goal: { guardReached: true, reason: 'Goal mode paused after 60 minutes.' } } } }, false);
    expect(outcome.detail).toBe('Goal mode paused after 60 minutes.');
    expect(outcome.tone).toBe('paused');
    expect(outcome.action).toBe('continue');
  });

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

  it('never shows a stalled or blocked goal as finished, and says why', () => {
    const blocked = runOutcome({ stopReason: 'end_turn', _meta: { pwr: { terminal: 'blocked', totalActions: 12, goal: { guardReached: true, verified: false, reason: 'dotnet test failed 3 times' } } } }, false);
    expect(blocked).toMatchObject({ tone: 'failed', action: 'continue', detail: 'dotnet test failed 3 times' });
    expect(blocked.text).toContain('blocked');
    const stalled = runOutcome({ stopReason: 'end_turn', _meta: { pwr: { terminal: 'stalled', totalActions: 20, goal: { guardReached: true, verified: false } } } }, false);
    expect(stalled).toMatchObject({ tone: 'paused', action: 'continue' });
    expect(stalled.text).toContain('no action');
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

describe('Goal completion evidence', () => {
  it('does not show a completed model reply as a verified goal when checks fail', () => {
    const outcome = runOutcome({ stopReason: 'end_turn', _meta: { pwr: {
      totalActions: 53, terminal: null,
      goal: { enabled: true, completed: true, verified: false, technicalPassed: false,
        verification: '0 of 1 full check(s) passing: npm test' },
    } } }, false);
    expect(outcome.text).toContain('without a verified completion');
    expect(outcome.tone).toBe('paused');
    expect(outcome.action).toBe('continue');
    expect(outcome.detail).toContain('0 of 1');
  });

  it('shows a changed acceptance contract as a stopped goal', () => {
    const outcome = runOutcome({ stopReason: 'end_turn', _meta: { pwr: {
      totalActions: 4, terminal: 'contract_changed',
      goal: { enabled: true, completed: true, verified: false, contractChanged: ['test/acceptance.ts'],
        verification: 'Acceptance artifact changed: test/acceptance.ts' },
    } } }, false);
    expect(outcome.text).toContain('acceptance checks changed');
    expect(outcome.tone).toBe('failed');
    expect(outcome.detail).toContain('test/acceptance.ts');
  });
});

describe('what an action did, in plain words', () => {
  const call = (title: string, text: string, extra: Partial<Entry> = {}) => entry('tool', { title, text, ...extra });

  it('says the verb and what it acted on, not the tool', () => {
    const diff = (oldText: string) => ({ path: 'src/app.ts', oldText, newText: 'x' });
    expect(actionPhrase(call('write_file src/app.ts', 'src/app.ts', { toolKind: 'edit', diff: diff('') }))).toEqual({ verb: 'Created', object: 'src/app.ts' });
    expect(actionPhrase(call('write_file src/app.ts', 'src/app.ts', { toolKind: 'edit', diff: diff('old') }))).toEqual({ verb: 'Wrote', object: 'src/app.ts' });
    expect(actionPhrase(call('apply_patch src/app.ts', 'src/app.ts', { toolKind: 'edit', diff: diff('old') })).verb).toBe('Edited');
    expect(actionPhrase(call('run_command', 'npm run build', { toolKind: 'execute' }))).toEqual({ verb: 'Ran', object: 'npm run build' });
    expect(actionPhrase(call('read_file src/a.ts', 'src/a.ts', { toolKind: 'read' }))).toEqual({ verb: 'Read', object: 'src/a.ts' });
  });

  it('leaves out the name of an argument and names the workspace', () => {
    expect(actionPhrase(call('make_directory', 'Arguments: path', { toolKind: 'edit', data: { path: 'src/Spese' } }))).toEqual({ verb: 'Created folder', object: 'src/Spese' });
    expect(actionPhrase(call('make_directory', 'Arguments: path', { toolKind: 'edit' }))).toEqual({ verb: 'Created folder', object: '' });
    expect(actionPhrase(call('list_tree', '.', { toolKind: 'search' }))).toEqual({ verb: 'Listed', object: 'the workspace' });
  });

  it('falls back to the kind of action, then to the tool, for one it does not know', () => {
    expect(actionPhrase(call('peek_file a.ts', 'a.ts', { toolKind: 'read' })).verb).toBe('Read');
    expect(actionPhrase(call('new_tool', 'x')).verb).toBe('new_tool');
  });
});
