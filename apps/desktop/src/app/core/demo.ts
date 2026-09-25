// A scripted conversation for reviewing the interface in a plain browser
// (`?demo`), where there is no core to talk to. Never used inside the app.
import { AgentStore } from './agent.store';

export function playDemo(store: AgentStore): void {
  store.useDemo(demoAnswer);
  store.coreState.set('ready');
  store.coreError.set('');
  store.workspace.set('/Users/you/projects/PWR/pwr-website');
  store.models.set(['lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit', 'lmstudio-community/Qwen3.8-27B-MLX-4bit']);
  store.model.set('lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit');
  store.context.set({ tokens: 262144, rationale: 'set by the model trained length', ceiling: 'Trained', setting: null });
  store.sessions.set([
    { sessionId: 'a', title: 'Build the PWR website', updatedAt: new Date().toISOString() },
    { sessionId: 'b', title: 'Fix the router and navigation', updatedAt: new Date(Date.now() - 86400000).toISOString() },
  ]);
  store.sessionId.set('a');
  store.usage.set({ used: 58_000, window: 262_144 });
  store.attachments.set(['/Users/you/Documents/CV.pdf', '/Users/you/projects/PWR']);
  const now = Date.now();
  store.timeline.set([
    { key: 'u1', kind: 'user', title: 'You', text: 'Create the official PWR website: modern, animated, with routing for Home, Project, Architecture, Documentation, About and Get started.', status: 'sent', attachments: ['/Users/you/Documents/CV.pdf', '/Users/you/projects/PWR'], at: now },
    { key: 't1', kind: 'thought', title: 'Thinking', text: 'The workspace is an Angular standalone app. The bootstrap is src/main.ts and the root template still shows the starter page. I will read the project documentation in ../docs before writing any claim about PWR, then define the routes and one navigation.', status: 'done', at: now },
    { key: 'c1', kind: 'tool', title: 'read_file ../README.md', text: '../README.md', status: 'done', toolKind: 'read', at: now },
    { key: 'c2', kind: 'tool', title: 'read_file ../docs/architecture.md', text: '../docs/architecture.md', status: 'done', toolKind: 'read', at: now },
    { key: 'c3', kind: 'tool', title: 'apply_replace src/app/app.routes.ts', text: 'src/app/app.routes.ts', status: 'done', toolKind: 'edit', diff: { path: 'src/app/app.routes.ts', oldText: "import { Routes } from '@angular/router';\n\nexport const routes: Routes = [];\n", newText: "import { Routes } from '@angular/router';\nimport { Home } from './pages/home';\nimport { Project } from './pages/project';\n\nexport const routes: Routes = [\n  { path: '', component: Home, title: 'PWR' },\n  { path: 'project', component: Project },\n  { path: '**', redirectTo: '' },\n];\n" }, at: now },
    { key: 'c4', kind: 'tool', title: 'run_command', text: 'npm run build', status: 'done', toolKind: 'execute', at: now },
    { key: 'g1', kind: 'generation', title: 'Generation', text: '', status: 'info', data: { promptTokens: 18_420, generatedTokens: 612, reasoningTokens: 240, answerTokens: 372, tokenAccounting: 'engine_tokenizer', promptEvalMs: 4_100, generationMs: 21_800, elapsedMs: 26_300, firstChunkMs: 4_350 }, at: now },
    { key: 'c5', kind: 'tool', title: 'Action', text: 'tool call replace_text did not match its declared schema: missing field `expected_hash`', status: 'failed', toolKind: 'other', at: now },
    { key: 'r0', kind: 'retry', title: 'malformed_call', text: 'replace_text: missing field `expected_hash`', status: 'done', data: { cause: 'malformed_call', attempt: 1, limit: 2 }, at: now },
    { key: 'v0', kind: 'recovery', title: 'Recovered', text: '', status: 'done', data: { retries: 1 }, at: now },
    { key: 'c6', kind: 'tool', title: 'run_command', text: 'npm test', status: 'done', toolKind: 'execute', at: now },
    { key: 'g2', kind: 'generation', title: 'Generation', text: '', status: 'info', data: { promptTokens: 21_050, generatedTokens: 488, reasoningTokens: 190, answerTokens: 298, tokenAccounting: 'engine_tokenizer', promptEvalMs: 1_900, generationMs: 17_400, elapsedMs: 19_500, firstChunkMs: 2_020 }, at: now },
    { key: 't2', kind: 'thought', title: 'Thinking', text: 'The build passed. Now the collision tests.\n\nThe player starts at `y = 50` and falls for `0.2 s`, so `vy = 1200 * 0.2 = 240` and `y = 50 + 48 = 98`. Its bottom is then at `98 + 32 = 130`, below the ground top at `96` and even below the tile bottom at `128` -- it went **through** the tile in one step.\n\nThat is tunneling. Sub-stepping `update` at `1/60 s` keeps each step under a tile height, so the overlap is always seen.', status: 'live', at: now },
  ]);
  store.changes.set([store.timeline()[4].diff!]);
  store.turnActive.set(true);
  const answer = 'The routes are in place and the build is running.\n\n- **Home** at `/` with the hero and a CTA\n- **Project**, **Architecture**, **Documentation**, **About**, **Get started**\n\nNext I will write the pages from `../docs`, then run the tests.';
  let shown = 0;
  const timer = setInterval(() => {
    shown = Math.min(answer.length, shown + 6);
    store.timeline.update((entries) => {
      const rest = entries.filter((entry) => entry.key !== 'r1');
      return [...rest, { key: 'r1', kind: 'reply', title: 'PWR', text: answer.slice(0, shown), status: shown < answer.length ? 'live' : 'done', at: now }];
    });
    store.lastEventAt.set(Date.now());
    if (shown >= answer.length) clearInterval(timer);
  }, 60);
}

const GIB = 1024 ** 3;

function fit(level: string, label: string, weights: number, window: number | null, explanation: string) {
  return {
    level, label, weightsBytes: weights, expectedBytes: weights + GIB, budgetBytes: 48 * GIB, windowTokens: window,
    gpuResident: null, explanation,
    assumptions: ['the engine costs about 1.0 GB beyond the weights', 'the host keeps a quarter of its memory (at least 8 GiB) for the system and other apps'],
  };
}

/** What the core would answer, for the demo's panels. */
function demoAnswer(method: string, params: any): any {
  switch (method) {
    case '_pwr/context':
      return {
        window: 262144, used: 58000, usedSource: 'engine', estimatedTokens: 61200,
        estimateBasis: 'characters divided by 4; not a tokenizer count',
        composition: { system: 2400, conversation: 9800, repository: 31000, toolResults: 12000, taskState: 1200, compactedMemory: 4800 },
        autoCompact: { enabled: true, thresholdPercent: params?.autoCompactPercent ?? 75, thresholdTokens: 196608, bounds: [50, 90], custom: false },
        lastCompaction: { trigger: 'automatic', at: new Date(Date.now() - 600000).toISOString(), tokensBefore: 201000, tokensAfter: 41000 },
      };
    case '_pwr/compact':
      return { compacted: true, note: 'summarised 42 earlier message(s) covering 3 request(s): 61200 → 14800 tokens', tokensBefore: 61200, tokensAfter: 14800 };
    case '_pwr/hardware':
      return {
        host: {
          platform: 'macos', osName: 'Darwin', osVersion: '26.0', architecture: 'arm64', cpu: 'Apple M2', appleChip: { name: 'Apple M2', generation: 2, tier: null },
          memory: { totalBytes: 16 * GIB, availableBytes: 7 * GIB, availableIsEstimate: true, unified: true },
          gpus: [{ name: 'Apple M2 GPU', vendor: 'apple', vramBytes: null, vramSource: 'unified memory', unifiedMemory: true }],
          disk: { path: '~/.pwr/models', freeBytes: 182 * GIB, totalBytes: 460 * GIB }, unknown: [],
        },
        backends: [
          { id: 'mlx', label: 'MLX (PWR engine)', format: 'mlx', active: true, available: true, detail: 'mlx-lm 0.28', modelsRoot: '~/.pwr/models' },
          { id: 'llama', label: 'llama.cpp', format: 'gguf', active: false, available: false, detail: 'llama.cpp backend is not configured: llama-server was not found.', modelsRoot: '~/.pwr/models' },
        ],
      };
    case '_pwr/catalog': {
      const variant = (id: string, q: string, size: number, f: any, extra: any = {}) => ({
        id, format: 'mlx', quantization: q, quantizationSource: 'config', files: [{ path: 'model.safetensors', bytes: size }], bytes: size,
        modelRef: id, fit: f, local: 'missing', localBytes: 0, installed: false, blocked: null, ...extra,
      });
      const entry = (repo: string, params: number, ctx: number, v: any) => ({
        repository: repo, name: repo.split('/')[1], author: repo.split('/')[0], url: 'https://huggingface.co/' + repo, revision: '0123456789abcdef0123456789abcdef01234567',
        format: 'mlx', backend: 'mlx', baseModel: 'Qwen/' + repo.split('/')[1].replace(/-MLX.*|-4bit|-8bit/g, ''), architecture: 'qwen3', architectureSource: 'config.json',
        parameters: params, parametersSource: 'safetensors metadata (Hub)', contextLength: ctx, contextSource: 'config.json', license: 'apache-2.0',
        downloads: 34567, likes: 42, gated: false, pipelineTag: 'text-generation', vision: false, variants: [v], bestFit: v.fit.level, notes: [],
      });
      if (params?.format === 'gguf') {
        return { format: 'gguf', backend: 'llama', activeBackend: 'mlx', modelsRoot: '~/.pwr/models', error: null, results: [] };
      }
      return {
        format: 'mlx', backend: 'mlx', activeBackend: 'mlx', modelsRoot: '~/.pwr/models', error: null,
        results: [
          entry('mlx-community/Qwen3-4B-4bit', 4.02e9, 40960, variant('mlx-community/Qwen3-4B-4bit', '4-bit', 2.26 * GIB,
            fit('should_fit', 'Should fit', 2.26 * GIB, 16384, 'Expected memory ~7.8 GB: 2.3 GB of weights, ~1.0 GB for the engine and 4.5 GB of context cache at 32k tokens. This machine has 16 GB of unified memory; after the reserve it leaves 8.0 GB, enough for a ~16k-token working window.'), { installed: true, local: 'present' })),
          entry('lmstudio-community/Qwen3.5-9B-MLX-4bit', 9.4e9, 262144, variant('lmstudio-community/Qwen3.5-9B-MLX-4bit', '4-bit', 5.6 * GIB,
            fit('tight_fit', 'Tight fit', 5.6 * GIB, 9216, 'Expected memory ~8.6 GB: 5.6 GB of weights, ~1.0 GB for the engine and 2.0 GB of context cache at 32k tokens. This machine has 16 GB of unified memory; after the reserve it leaves 8.0 GB, enough for a ~9k-token working window. Long conversations will be compacted often.'))),
          entry('lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit', 35.1e9, 262144, variant('lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit', '4-bit', 19 * GIB,
            fit('not_recommended', 'Not recommended', 19 * GIB, null, 'This machine has 16 GB of unified memory; after the reserve that leaves 8.0 GB for a model, less than its weights and the engine need.'))),
        ],
      };
    }
    case '_pwr/local_models':
      return {
        activeBackend: 'mlx',
        models: [
          { modelRef: 'lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit', format: 'mlx', backend: 'mlx', path: '~/.pwr/models/lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit', bytes: 19 * GIB, files: 13, partial: false, inUse: true, usable: true },
          { modelRef: 'mlx-community/Qwen3-4B-4bit', format: 'mlx', backend: 'mlx', path: '~/.pwr/models/mlx-community/Qwen3-4B-4bit', bytes: 2.3 * GIB, files: 9, partial: false, inUse: false, usable: true },
        ],
      };
    case '_pwr/model_delete':
      return { modelRef: params.modelRef, freedBytes: 2.3 * GIB, removed: [] };
    case '_pwr/download':
      return { modelRef: params.repository, backend: 'mlx', ready: true, nextStep: null };
    case '_pwr/models':
      return {
        installed: [
          'lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit',
          'lmstudio-community/Qwen3.8-27B-MLX-4bit',
        ],
        model: 'lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit',
        contextTokens: 262144,
        reasoningEffort: params.reasoningEffort ?? 'medium',
        reasoning: {
          effort: params.reasoningEffort ?? 'medium',
          applies: true,
          control: 'budget',
          budgets: { low: 1024, medium: 4096, high: 8192 },
          budgetSource: 'conservative',
        },
        compatibility: {
          status: 'provisional',
          confidence: 'untested',
          summary: 'New model detected',
          reasons: [],
          features: { chat: true, agent: true, note: null },
          checks: [],
          capabilities: [],
          recalibrate: true,
          acknowledged: !!params.acknowledgeProvisional,
        },
      };
    default:
      return {};
  }
}
