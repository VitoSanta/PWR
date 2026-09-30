import { vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { AgentStore } from './agent.store';

describe('AgentStore context', () => {
  let store: AgentStore;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    store = TestBed.inject(AgentStore);
    store.sessionId.set('s1');
  });

  it('requires authorization for each named acceptance artifact before offering continuation', async () => {
    store.runOutcome.set({ terminal: 'contract_changed', text: 'Review required', detail: null, action: null, tone: 'failed', acceptanceChanges: ['tests/a.rs', 'tests/b.rs'] });
    const request = vi.spyOn(store as any, 'request').mockResolvedValue({ allowed: true });
    await store.reviewAcceptanceChanges();
    expect(request.mock.calls).toEqual([
      ['_pwr/acceptance_authorize', { sessionId: 's1', path: 'tests/a.rs' }],
      ['_pwr/acceptance_authorize', { sessionId: 's1', path: 'tests/b.rs' }],
    ]);
    expect(store.runOutcome()?.action).toBe('continue');
  });

  it('keeps changed acceptance evidence blocked after a refusal', async () => {
    store.runOutcome.set({ terminal: 'contract_changed', text: 'Review required', detail: null, action: null, tone: 'failed', acceptanceChanges: ['tests/a.rs'] });
    vi.spyOn(store as any, 'request').mockResolvedValue({ allowed: false });
    await store.reviewAcceptanceChanges();
    expect(store.runOutcome()?.tone).toBe('failed');
    expect(store.runOutcome()?.action).toBeNull();
  });

  it('shows an automatic compaction in the conversation', () => {
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/compacted',
      params: {
        sessionId: 's1',
        trigger: 'automatic',
        note: 'summarised 6 earlier message(s) covering 2 request(s): 900 → 200 tokens',
      },
    });
    const notice = store.timeline().at(-1)!;
    expect(notice.kind).toBe('notice');
    expect(notice.title).toBe('Context compacted automatically');
    expect(notice.text).toContain('Summarised 6 earlier message(s)');
    expect(notice.text).toContain('open errors');
  });

  it('ignores a compaction of another session', () => {
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/compacted',
      params: { sessionId: 'other', trigger: 'manual', note: 'x' },
    });
    expect(store.timeline().length).toBe(0);
  });

  it('keeps the engine count for the context indicator', () => {
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/usage',
      params: { used: 54_000, window: 128_000 },
    });
    expect(store.usage()).toEqual({ used: 54_000, window: 128_000, estimated: false });
    store.receive({ jsonrpc: '2.0', method: '_pwr/usage', params: { used: 1, window: 0 } });
    expect(store.usage()).toEqual({ used: 54_000, window: 128_000, estimated: false });
  });

  it('keeps how far the engine has read the prompt, for this session only', () => {
    store.sessionId.set('s1');
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/model_progress',
      params: { sessionId: 's1', prefill: { processed: 4_096, total: 20_000 } },
    });
    expect(store.prefill()).toMatchObject({ processed: 4_096, total: 20_000 });
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/model_progress',
      params: { sessionId: 'other', prefill: { processed: 9, total: 10 } },
    });
    store.receive({ jsonrpc: '2.0', method: '_pwr/model_progress', params: { sessionId: 's1' } });
    expect(store.prefill()).toMatchObject({ processed: 4_096, total: 20_000 });
  });

  it("numbers the person's message with the turn the core started for it", () => {
    store.sessionId.set('s1');
    store.timeline.set([{ key: 'u1', kind: 'user', title: 'You', text: 'build it', status: 'sent', at: 1 }]);
    store.receive({ jsonrpc: '2.0', method: '_pwr/turn_started', params: { sessionId: 's1', turn: 3 } });
    expect(store.timeline()[0].turn).toBe(3);
    // Another session's turn is not this one's.
    store.timeline.set([{ key: 'u2', kind: 'user', title: 'You', text: 'x', status: 'sent', at: 2 }]);
    store.receive({ jsonrpc: '2.0', method: '_pwr/turn_started', params: { sessionId: 'other', turn: 4 } });
    expect(store.timeline()[0].turn).toBeUndefined();
  });

  it('marks restored messages so the UI does not invent a turn duration', () => {
    for (const [kind, content] of [
      ['user_message_chunk', 'a saved request'],
      ['agent_message_chunk', 'a saved reply'],
    ]) {
      store.receive({
        jsonrpc: '2.0',
        method: 'session/update',
        params: { update: { sessionUpdate: kind, content: { type: 'text', text: content }, _meta: { pwr: { replay: true } } } },
      });
    }
    expect(store.timeline().map((entry) => entry.replayed)).toEqual([true, true]);
  });

  it('keeps the model recovered for a restored turn', () => {
    store.receive({
      jsonrpc: '2.0', method: 'session/update',
      params: { update: { sessionUpdate: 'user_message_chunk', content: { text: 'saved request' }, _meta: { pwr: { replay: true, model: 'gemma-4' } } } },
    });
    expect(store.timeline()[0].modelName).toBe('gemma 4');
  });

  it('edits, reorders and drops queued messages before they are sent', () => {
    store.queue.set(['first', 'second', 'third']);
    store.editQueued(1, 'second, reworded');
    store.moveQueued(2, -1);
    expect(store.queue()).toEqual(['first', 'third', 'second, reworded']);
    store.moveQueued(0, -1);
    expect(store.queue()[0]).toBe('first');
    store.editQueued(0, '   ');
    expect(store.queue()).toEqual(['third', 'second, reworded']);
  });

  it('follows the window while a reply streams, until the engine counts it', () => {
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/usage',
      params: { used: 20_000, window: 262_144, estimated: true },
    });
    expect(store.usage()?.estimated).toBe(true);
    const chunk = (sessionUpdate: string, text: string, live = false) =>
      store.receive({
        jsonrpc: '2.0',
        method: 'session/update',
        params: {
          update: { sessionUpdate, content: { text }, ...(live ? { _meta: { pwr: { live: true } } } : {}) },
        },
      });
    chunk('agent_thought_chunk', 'x'.repeat(400));
    chunk('agent_message_chunk', 'y'.repeat(400), true);
    expect(store.streamedTokens()).toBe(200);
    store.receive({ jsonrpc: '2.0', method: '_pwr/usage', params: { used: 20_190, window: 262_144 } });
    expect(store.streamedTokens()).toBe(0);
    expect(store.usage()).toEqual({ used: 20_190, window: 262_144, estimated: false });
  });

  it('replaces a streamed answer with the final answer and its check verdict', () => {
    const message = (text: string, live: boolean) => store.receive({
      jsonrpc: '2.0', method: 'session/update',
      params: { update: { sessionUpdate: 'agent_message_chunk', content: { text }, ...(live ? { _meta: { pwr: { live: true } } } : {}) } },
    });
    message('Wrote result.txt.', true);
    message('Wrote result.txt.\n\nIndependent verification unavailable: this workspace declares no automated checks.', false);
    expect(store.timeline().filter((entry) => entry.kind === 'reply')).toHaveLength(1);
    expect(store.timeline()[0]).toMatchObject({
      status: 'done',
      text: 'Wrote result.txt.\n\nIndependent verification unavailable: this workspace declares no automated checks.',
    });
  });

  it('routes extension notifications to their listeners only', () => {
    const seen: any[] = [];
    const stop = store.on('_pwr/download_progress', (params) => seen.push(params));
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/download_progress',
      params: { downloadId: 'd', state: { state: 'preparing' } },
    });
    store.receive({ jsonrpc: '2.0', method: '_pwr/other', params: {} });
    stop();
    store.receive({
      jsonrpc: '2.0',
      method: '_pwr/download_progress',
      params: { downloadId: 'd2' },
    });
    expect(seen).toEqual([{ downloadId: 'd', state: { state: 'preparing' } }]);
  });

  it('compacts on request through the core and reports nothing to fold', async () => {
    const asked: string[] = [];
    store.useDemo((method) => {
      asked.push(method);
      if (method === '_pwr/compact')
        return {
          compacted: false,
          reason: 'Nothing older than the most recent exchanges to summarise yet.',
        };
      return {
        window: 8192,
        used: 10,
        usedSource: 'estimate',
        composition: {},
        autoCompact: { thresholdPercent: 75 },
      };
    });
    await store.compactNow();
    expect(asked).toEqual(['_pwr/compact', '_pwr/context']);
    expect(store.timeline().at(-1)!.title).toBe('Nothing to compact');
    expect(store.compacting()).toBe(false);
  });

  it('does not compact while a turn runs', async () => {
    const asked: string[] = [];
    store.useDemo((method) => asked.push(method));
    store.turnActive.set(true);
    await store.compactNow();
    expect(asked).toEqual([]);
  });
});

describe('AgentStore turn events', () => {
  let store: AgentStore;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    store = TestBed.inject(AgentStore);
    store.sessionId.set('s1');
  });

  const event = (params: any) => store.receive({ jsonrpc: '2.0', method: '_pwr/turn_event', params: { sessionId: 's1', ...params } });

  it('keeps a retry running until the turn recovers', () => {
    event({ event: 'retry', cause: 'reply_fault', attempt: 1, limit: 2, detail: 'tool calls were not valid JSON' });
    expect(store.timeline()[0]).toMatchObject({ kind: 'retry', status: 'running', data: { cause: 'reply_fault', attempt: 1 } });
    event({ event: 'recovered', retries: 1 });
    expect(store.timeline().map((entry) => [entry.kind, entry.status])).toEqual([
      ['retry', 'done'],
      ['recovery', 'done'],
    ]);
  });

  it('records a generation for the runtime figures', () => {
    event({ event: 'generation', promptTokens: 10, generatedTokens: 5, generationMs: 100 });
    expect(store.timeline()[0]).toMatchObject({ kind: 'generation', data: { generatedTokens: 5 } });
  });

  it("turns the core's stop reason into the run's state, not the answer's prose", () => {
    const said = 'three turns running produced nothing this backend could use';
    event({ event: 'retry', cause: 'reply_fault', attempt: 2, limit: 2, detail: 'x' });
    store.receive({
      jsonrpc: '2.0',
      method: 'session/update',
      params: { update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: `Read the files.\n\n${said}` } } },
    });
    (store as any).finish({ stopReason: 'end_turn', _meta: { pwr: { terminal: 'protocol', actions: 2, stoppedBecause: said } } });
    const kinds = store.timeline().map((entry) => entry.kind);
    expect(kinds).toEqual(['retry', 'reply', 'stop']);
    expect(store.timeline()[0].status).toBe('failed');
    expect(store.timeline()[1].text).toBe('Read the files.');
    expect(store.timeline()[2].text).toBe(said);
    expect(store.runOutcome()?.action).toBe('retry');
  });
});
