import { TestBed } from '@angular/core/testing';
import { AgentStore } from './agent.store';

describe('AgentStore context', () => {
  let store: AgentStore;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    store = TestBed.inject(AgentStore);
    store.sessionId.set('s1');
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
