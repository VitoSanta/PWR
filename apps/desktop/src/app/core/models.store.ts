import { Injectable, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import { DownloadView, HardwareInfo, LinePack } from './model';

/**
 * The Model Manager's state: the Gemma 4 line as this Mac sees it. Which
 * packs exist, whether each is here, how much of the 262,144-token context
 * this machine holds for it, and a download's state all come from the core
 * (`_pwr/local_models`, `_pwr/download`); this only asks and draws.
 */
@Injectable({ providedIn: 'root' })
export class ModelsStore {
  private readonly agent = inject(AgentStore);

  readonly open = signal(false);
  readonly status = signal<'idle' | 'loading' | 'ready' | 'error'>('idle');
  readonly error = signal('');
  readonly packs = signal<LinePack[]>([]);
  readonly memoryBytes = signal<number | null>(null);
  readonly contextTokens = signal(262144);
  readonly modelsRoot = signal('');
  /** Downloads by model reference. */
  readonly downloads = signal<Record<string, DownloadView>>({});
  readonly pendingDelete = signal<LinePack | null>(null);
  readonly deleting = signal(false);
  readonly deleteError = signal('');
  /** This machine and its engine, for the run metrics. */
  readonly hardware = signal<HardwareInfo | null>(null);

  private listening = false;

  show(): void {
    this.open.set(true);
    this.listen();
    void this.load();
  }

  close(): void {
    this.open.set(false);
    this.pendingDelete.set(null);
  }

  async load(): Promise<void> {
    this.status.set('loading');
    this.error.set('');
    try {
      const reply = await this.agent.call('_pwr/local_models', { cwd: this.agent.workspace() });
      this.packs.set(reply.packs ?? []);
      this.memoryBytes.set(reply.memoryBytes ?? null);
      this.contextTokens.set(reply.contextTokens ?? 262144);
      this.modelsRoot.set(reply.modelsRoot ?? '');
      this.status.set('ready');
    } catch (error) {
      this.error.set(String(error).replace(/^Error: /, ''));
      this.status.set('error');
    }
  }

  async loadHardware(): Promise<void> {
    try {
      this.hardware.set(await this.agent.call('_pwr/hardware', {}));
    } catch {
      // The metrics show no engine line; nothing else depends on it.
    }
  }

  /** Downloads a pack from the registry: pinned revision, every file checked. */
  async download(pack: LinePack): Promise<void> {
    if (!pack.artifact) return;
    const current = this.downloads()[pack.model];
    if (current && !isTerminal(current)) return;
    const downloadId = `dl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    this.put(pack.model, {
      downloadId,
      modelRef: pack.model,
      format: 'mlx',
      totalBytes: pack.weightsBytes,
      state: { state: 'preparing' },
    });
    try {
      const reply = await this.agent.call('_pwr/download', {
        cwd: this.agent.workspace(),
        downloadId,
        artifact: pack.artifact,
      });
      this.patch(pack.model, {
        state: { state: 'completed', total: pack.weightsBytes },
        ready: reply.ready,
        nextStep: reply.nextStep ?? null,
      });
      await Promise.all([this.agent.refreshModels(), this.load()]);
    } catch (error) {
      const view = this.downloads()[pack.model];
      if (view && !isTerminal(view)) {
        this.patch(pack.model, {
          state: { state: 'failed', kind: 'unexpected', message: String(error), bytes: 0 },
        });
      }
    }
  }

  /** Pausing cancels the request and keeps the verified partial files to resume from. */
  pause(pack: LinePack): void {
    const view = this.downloads()[pack.model];
    if (view && !isTerminal(view)) this.agent.notify('_pwr/download_cancel', { downloadId: view.downloadId });
  }

  async use(pack: LinePack): Promise<void> {
    await this.agent.selectModel(pack.model);
    await this.load();
    this.close();
  }

  /** Asks before deleting: the files go, and cannot be brought back. */
  askDelete(pack: LinePack): void {
    this.deleteError.set('');
    this.pendingDelete.set(pack);
  }

  cancelDelete(): void {
    this.pendingDelete.set(null);
    this.deleteError.set('');
  }

  async confirmDelete(): Promise<void> {
    const target = this.pendingDelete();
    if (!target || this.deleting()) return;
    this.deleting.set(true);
    this.deleteError.set('');
    try {
      await this.agent.call('_pwr/model_delete', {
        cwd: this.agent.workspace(),
        modelRef: target.model,
        format: 'mlx',
      });
      this.pendingDelete.set(null);
      this.downloads.update((all) => Object.fromEntries(Object.entries(all).filter(([model]) => model !== target.model)));
      await Promise.all([this.agent.refreshModels(), this.load()]);
    } catch (error) {
      this.deleteError.set(String(error).replace(/^Error: /, ''));
    } finally {
      this.deleting.set(false);
    }
  }

  private listen(): void {
    if (this.listening) return;
    this.listening = true;
    this.agent.on('_pwr/download_progress', (params) => {
      const entry = Object.entries(this.downloads()).find(([, view]) => view.downloadId === params.downloadId);
      if (!entry || !params.state) return;
      const [model, view] = entry;
      // A terminal state is final, as the core's own state machine has it.
      if (view.state.state === 'completed') return;
      this.patch(model, { state: params.state, file: params.file ?? view.file });
    });
  }

  private put(model: string, view: DownloadView): void {
    this.downloads.update((all) => ({ ...all, [model]: view }));
  }

  private patch(model: string, change: Partial<DownloadView>): void {
    this.downloads.update((all) => (all[model] ? { ...all, [model]: { ...all[model], ...change } } : all));
  }
}

export function isTerminal(view: DownloadView): boolean {
  return ['completed', 'failed', 'cancelled'].includes(view.state.state);
}
