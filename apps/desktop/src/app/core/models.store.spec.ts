import { TestBed } from '@angular/core/testing';
import { AgentStore } from './agent.store';
import { CatalogEntry, CatalogVariant } from './model';
import { ModelsStore, isTerminal } from './models.store';

const variant: CatalogVariant = {
  id: 'mlx',
  format: 'mlx',
  quantization: '4-bit',
  quantizationSource: 'config',
  files: [],
  bytes: 1000,
  modelRef: 'a/b',
  local: 'missing',
  localBytes: 0,
  installed: false,
  blocked: null,
  fit: {
    level: 'should_fit',
    label: 'Should fit',
    weightsBytes: 1000,
    expectedBytes: 2000,
    budgetBytes: 8000,
    windowTokens: 16384,
    gpuResident: null,
    explanation: '',
    assumptions: [],
  },
};
const entry = {
  repository: 'a/b',
  revision: '0123456789abcdef0123456789abcdef01234567',
  variants: [variant],
} as unknown as CatalogEntry;

describe('ModelsStore downloads', () => {
  let agent: AgentStore;
  let models: ModelsStore;
  let finish: (value: any) => void;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    agent = TestBed.inject(AgentStore);
    models = TestBed.inject(ModelsStore);
    models.results.set([entry]);
  });

  function core(downloadReply: () => Promise<any>) {
    agent.useDemo((method) => {
      if (method === '_pwr/download') return downloadReply();
      if (method === '_pwr/models') return { installed: ['a/b'], model: null };
      if (method === '_pwr/catalog') return { format: 'mlx', results: [entry], error: null };
      return {};
    });
  }

  it('follows the core state machine to completion and marks the model available', async () => {
    core(() => new Promise((resolve) => (finish = resolve)));
    models.show();
    const started = models.download(entry, variant);
    await new Promise((r) => setTimeout(r, 400));
    const id = models.downloads()['a/b@mlx'].downloadId;
    agent.receive({
      jsonrpc: '2.0',
      method: '_pwr/download_progress',
      params: { downloadId: id, state: { state: 'downloading', bytes: 400, total: 1000 } },
    });
    expect(models.downloads()['a/b@mlx'].state).toEqual({
      state: 'downloading',
      bytes: 400,
      total: 1000,
    });
    finish({ modelRef: 'a/b', ready: true, nextStep: null });
    await started;
    expect(models.downloads()['a/b@mlx'].state.state).toBe('completed');
    expect(models.results()[0].variants[0].installed).toBe(true);
    // A late report cannot revive it.
    agent.receive({
      jsonrpc: '2.0',
      method: '_pwr/download_progress',
      params: { downloadId: id, state: { state: 'downloading', bytes: 900, total: 1000 } },
    });
    expect(models.downloads()['a/b@mlx'].state.state).toBe('completed');
  });

  it('keeps the failure the core reported', async () => {
    core(() => Promise.reject(new Error('download needs 1000 bytes')));
    models.show();
    const started = models.download(entry, variant);
    await new Promise((r) => setTimeout(r, 400));
    const id = models.downloads()['a/b@mlx'].downloadId;
    agent.receive({
      jsonrpc: '2.0',
      method: '_pwr/download_progress',
      params: {
        downloadId: id,
        state: {
          state: 'failed',
          kind: 'insufficient_disk',
          message: 'download needs 1000 bytes',
          bytes: 0,
        },
      },
    });
    await started;
    const view = models.downloads()['a/b@mlx'];
    expect(view.state).toEqual({
      state: 'failed',
      kind: 'insufficient_disk',
      message: 'download needs 1000 bytes',
      bytes: 0,
    });
    expect(isTerminal(view)).toBe(true);
  });

  it('does not start a second download of the same variant', async () => {
    let calls = 0;
    core(() => {
      calls++;
      return new Promise(() => {});
    });
    void models.download(entry, variant);
    void models.download(entry, variant);
    await new Promise((r) => setTimeout(r, 400));
    expect(calls).toBe(1);
  });

  it('loads every requested catalog page and keeps its next cursor', async () => {
    const second = { ...entry, repository: 'c/d' };
    const requested: Array<string | undefined> = [];
    agent.useDemo((method, params) => {
      if (method !== '_pwr/catalog') return {};
      requested.push(params.cursor);
      return params.cursor
        ? { format: 'mlx', results: [second], nextCursor: null, error: null }
        : { format: 'mlx', results: [entry], nextCursor: 'page-2', error: null };
    });
    await models.search();
    expect(models.nextCursor()).toBe('page-2');
    await models.loadMore();
    expect(requested).toEqual([undefined, 'page-2']);
    expect(models.results().map((model) => model.repository)).toEqual(['a/b', 'c/d']);
    expect(models.nextCursor()).toBeNull();
  });
});
