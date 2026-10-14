import { TestBed } from '@angular/core/testing';
import { AgentStore } from './agent.store';
import { LinePack } from './model';
import { ModelsStore, isTerminal } from './models.store';

const pack: LinePack = {
  model: 'mlx-community/gemma-4-26b-a4b-it-4bit',
  label: 'Gemma 4 26B',
  tier: 'middle',
  tierMemoryGib: 24,
  weightsBytes: 15_341_205_776,
  artifact: 'hf-gemma-4-26b-a4b-it-4bit',
  installed: false,
  partial: false,
  bytesOnDisk: 0,
  inUse: false,
  windowTokens: 262144,
  fullContext: true,
  neededBytes: 30_000_000_000,
  assistant: { model: 'mlx-community/gemma-4-26B-A4B-it-assistant-bf16', installed: false },
};

/** The demo core answers after 350 ms; wait for it to have been asked. */
const answered = () => new Promise((resolve) => setTimeout(resolve, 400));

describe('ModelsStore', () => {
  let agent: AgentStore;
  let models: ModelsStore;
  let asked: { method: string; params: any }[];

  function core(downloadReply: () => Promise<any>, installed = false) {
    asked = [];
    agent.useDemo((method, params) => {
      asked.push({ method, params });
      if (method === '_pwr/download') return downloadReply();
      if (method === '_pwr/local_models')
        return { memoryBytes: 64 * 2 ** 30, contextTokens: 262144, modelsRoot: '~/.pwr/models', packs: [{ ...pack, installed }] };
      if (method === '_pwr/models') return { installed: installed ? [pack.model] : [], model: null };
      return {};
    });
  }

  beforeEach(() => {
    TestBed.configureTestingModule({});
    agent = TestBed.inject(AgentStore);
    models = TestBed.inject(ModelsStore);
  });

  it('lists the line the core describes', async () => {
    core(() => Promise.resolve({}));
    await models.load();
    expect(models.packs().map((p) => p.model)).toEqual([pack.model]);
    expect(models.contextTokens()).toBe(262144);
    expect(models.status()).toBe('ready');
  });

  it('downloads a pack by its registry entry and follows it to completion', async () => {
    let finish: (value: any) => void = () => {};
    core(() => new Promise((resolve) => (finish = resolve)));
    const started = models.download(pack);
    expect(models.downloads()[pack.model].state.state).toBe('preparing');
    await answered();
    const request = asked.find((call) => call.method === '_pwr/download')!;
    expect(request.params.artifact).toBe(pack.artifact);
    expect(request.params.repository).toBeUndefined();
    finish({ modelRef: pack.model, ready: true });
    await started;
    expect(models.downloads()[pack.model].state.state).toBe('completed');
    expect(isTerminal(models.downloads()[pack.model])).toBe(true);
  });

  it('keeps a failure the transport reported', async () => {
    core(() => Promise.reject(new Error('network down')));
    await models.download(pack);
    const state = models.downloads()[pack.model].state;
    expect(state.state).toBe('failed');
  });

  it('does not start a second download of the same pack', async () => {
    let finish: (value: any) => void = () => {};
    core(() => new Promise((resolve) => (finish = resolve)));
    const first = models.download(pack);
    void models.download(pack);
    await answered();
    expect(asked.filter((call) => call.method === '_pwr/download').length).toBe(1);
    finish({ ready: true });
    await first;
  });

  it('downloads nothing for a pack with no registry entry', async () => {
    core(() => Promise.resolve({}));
    await models.download({ ...pack, artifact: null });
    expect(asked.some((call) => call.method === '_pwr/download')).toBe(false);
  });
});
