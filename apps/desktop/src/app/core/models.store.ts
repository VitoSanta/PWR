import { Injectable, computed, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import {
  CatalogEntry,
  CatalogFilters,
  CatalogVariant,
  DownloadView,
  HardwareInfo,
  LocalModel,
} from './model';

/**
 * The Model Manager's state. Everything it shows -- variants, fit, what is on
 * disk, a download's state -- is computed by the core (`_pwr/hardware`,
 * `_pwr/catalog`, `_pwr/download`); this only asks and draws.
 */
@Injectable({ providedIn: 'root' })
export class ModelsStore {
  private readonly agent = inject(AgentStore);

  readonly open = signal(false);
  readonly hardware = signal<HardwareInfo | null>(null);
  readonly hardwareError = signal('');

  readonly query = signal('');
  /** `null` until the first search: the core then picks the active engine's format. */
  readonly format = signal<'mlx' | 'gguf' | null>(null);
  readonly filters = signal<CatalogFilters>({ compatibleOnly: true });

  readonly status = signal<'idle' | 'loading' | 'ready' | 'error'>('idle');
  readonly results = signal<CatalogEntry[]>([]);
  readonly nextCursor = signal<string | null>(null);
  readonly loadingMore = signal(false);
  readonly loadMoreError = signal('');
  /** The Hub could not be searched: offline, rate-limited, … */
  readonly error = signal<{ kind: string; message: string } | null>(null);
  readonly activeBackend = signal('');
  readonly modelsRoot = signal('');

  /** Discover (the Hub) or the models already on this machine. */
  readonly tab = signal<'discover' | 'local'>('discover');
  readonly local = signal<LocalModel[]>([]);
  readonly localStatus = signal<'idle' | 'loading' | 'ready' | 'error'>('idle');
  readonly localError = signal('');
  /** The model waiting for "Delete" to be confirmed. */
  readonly pendingDelete = signal<{
    modelRef: string;
    format: 'mlx' | 'gguf';
    bytes: number;
    path: string;
  } | null>(null);
  readonly deleting = signal(false);
  readonly deleteError = signal('');

  /** Downloads by `repository@variant`. */
  readonly downloads = signal<Record<string, DownloadView>>({});

  /** The engine for the format on screen, and whether it can run here. */
  readonly backend = computed(() => {
    const format = this.format();
    return this.hardware()?.backends.find((backend) => backend.format === format) ?? null;
  });

  private searchSequence = 0;
  private listening = false;
  private typing?: ReturnType<typeof setTimeout>;

  show(): void {
    this.open.set(true);
    this.listen();
    if (!this.hardware()) void this.loadHardware();
    if (this.status() === 'idle') void this.search();
  }

  close(): void {
    this.open.set(false);
    this.pendingDelete.set(null);
  }

  showTab(tab: 'discover' | 'local'): void {
    this.tab.set(tab);
    if (tab === 'local') void this.loadLocal();
  }

  /** The models on this machine, for every engine. */
  async loadLocal(): Promise<void> {
    this.localStatus.set('loading');
    this.localError.set('');
    try {
      const reply = await this.agent.call('_pwr/local_models', { cwd: this.agent.workspace() });
      this.local.set(reply.models ?? []);
      this.localStatus.set('ready');
    } catch (error) {
      this.localError.set(String(error));
      this.localStatus.set('error');
    }
  }

  /** Asks before deleting: the files go, and cannot be brought back. */
  askDelete(target: {
    modelRef: string;
    format: 'mlx' | 'gguf';
    bytes: number;
    path?: string;
  }): void {
    this.deleteError.set('');
    this.pendingDelete.set({ ...target, path: target.path ?? '' });
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
        modelRef: target.modelRef,
        format: target.format,
      });
      this.pendingDelete.set(null);
      this.forget(target.modelRef);
      await Promise.all([this.agent.refreshModels(), this.loadLocal()]);
    } catch (error) {
      this.deleteError.set(String(error).replace(/^Error: /, ''));
    } finally {
      this.deleting.set(false);
    }
  }

  /** A deleted model is neither on disk nor available in the cards any more. */
  private forget(modelRef: string): void {
    this.results.update((entries) =>
      entries.map((entry) => ({
        ...entry,
        variants: entry.variants.map((variant) =>
          variant.modelRef === modelRef
            ? { ...variant, local: 'missing', localBytes: 0, installed: false }
            : variant,
        ),
      })),
    );
    this.downloads.update((all) =>
      Object.fromEntries(Object.entries(all).filter(([, view]) => view.modelRef !== modelRef)),
    );
    this.local.update((models) => models.filter((model) => model.modelRef !== modelRef));
  }

  async loadHardware(): Promise<void> {
    this.hardwareError.set('');
    try {
      this.hardware.set(await this.agent.call('_pwr/hardware', {}));
    } catch (error) {
      this.hardwareError.set(String(error));
    }
  }

  setFormat(format: 'mlx' | 'gguf'): void {
    if (this.format() === format) return;
    this.format.set(format);
    void this.search();
  }

  setFilters(change: Partial<CatalogFilters>): void {
    this.filters.update((current) => ({ ...current, ...change }));
    void this.search();
  }

  /** The search box: searches once typing pauses, as a search box should. */
  typeQuery(text: string): void {
    this.query.set(text);
    this.searchSoon();
  }

  /** A text filter (family, quantization), applied once typing pauses. */
  typeFilter(change: Partial<CatalogFilters>): void {
    this.filters.update((current) => ({ ...current, ...change }));
    this.searchSoon();
  }

  /** Search now: Enter or the button, without waiting for the pause. */
  searchNow(): void {
    clearTimeout(this.typing);
    void this.search();
  }

  private searchSoon(): void {
    clearTimeout(this.typing);
    ++this.searchSequence;
    this.status.set('loading');
    this.nextCursor.set(null);
    this.loadingMore.set(false);
    this.typing = setTimeout(() => void this.search(), 400);
  }

  /** Searches the Hub through the core. A slower earlier answer never replaces a later one. */
  async search(): Promise<void> {
    const sequence = ++this.searchSequence;
    this.status.set('loading');
    this.error.set(null);
    this.nextCursor.set(null);
    this.loadMoreError.set('');
    this.loadingMore.set(false);
    try {
      const reply = await this.agent.call('_pwr/catalog', {
        cwd: this.agent.workspace(),
        query: this.query(),
        format: this.format() ?? undefined,
        filters: clean(this.filters()),
      });
      if (sequence !== this.searchSequence) return;
      this.format.set(reply.format ?? this.format());
      this.activeBackend.set(reply.activeBackend ?? '');
      this.modelsRoot.set(reply.modelsRoot ?? '');
      this.results.set(reply.results ?? []);
      this.nextCursor.set(reply.nextCursor ?? null);
      this.error.set(reply.error ?? null);
      this.status.set(reply.error ? 'error' : 'ready');
    } catch (error) {
      if (sequence !== this.searchSequence) return;
      this.results.set([]);
      this.nextCursor.set(null);
      this.error.set({ kind: 'unexpected', message: String(error) });
      this.status.set('error');
    }
  }

  /** Appends the next Hub page using the cursor returned for this search. */
  async loadMore(): Promise<void> {
    const cursor = this.nextCursor();
    if (!cursor || this.status() !== 'ready' || this.loadingMore()) return;
    const sequence = this.searchSequence;
    this.loadingMore.set(true);
    this.loadMoreError.set('');
    try {
      const reply = await this.agent.call('_pwr/catalog', {
        cwd: this.agent.workspace(),
        query: this.query(),
        format: this.format() ?? undefined,
        filters: clean(this.filters()),
        cursor,
      });
      if (sequence !== this.searchSequence) return;
      if (reply.error) {
        this.loadMoreError.set(reply.error.message ?? 'The next page could not be loaded.');
        return;
      }
      const seen = new Set(this.results().map((entry) => entry.repository));
      const more = (reply.results ?? []).filter((entry: CatalogEntry) => !seen.has(entry.repository));
      this.results.update((current) => [...current, ...more]);
      this.nextCursor.set(reply.nextCursor ?? null);
    } catch (error) {
      if (sequence === this.searchSequence) this.loadMoreError.set(String(error));
    } finally {
      if (sequence === this.searchSequence) this.loadingMore.set(false);
    }
  }

  key(entry: CatalogEntry, variant: CatalogVariant): string {
    return `${entry.repository}@${variant.id}`;
  }

  async download(entry: CatalogEntry, variant: CatalogVariant): Promise<void> {
    if (!entry.revision) return;
    const key = this.key(entry, variant);
    const current = this.downloads()[key];
    if (current && !isTerminal(current)) return;
    const downloadId = `dl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    this.put(key, { downloadId, state: { state: 'preparing' } });
    try {
      const reply = await this.agent.call('_pwr/download', {
        cwd: this.agent.workspace(),
        downloadId,
        repository: entry.repository,
        revision: entry.revision,
        variant: variant.id,
        format: variant.format,
      });
      this.patch(key, {
        state: { state: 'completed', total: variant.bytes },
        modelRef: reply.modelRef,
        ready: reply.ready,
        nextStep: reply.nextStep ?? null,
      });
      // The engine lists it now: the model picker and the cards pick it up.
      await this.agent.refreshModels();
      this.markInstalled(entry.repository, variant.id, reply.ready === true);
    } catch (error) {
      // The core's last `download_progress` carries the state and its kind;
      // this only covers a failure before any was sent.
      const view = this.downloads()[key];
      if (view && !isTerminal(view)) {
        this.patch(key, {
          state: { state: 'failed', kind: 'unexpected', message: String(error), bytes: 0 },
        });
      }
    }
  }

  cancel(entry: CatalogEntry, variant: CatalogVariant): void {
    const view = this.downloads()[this.key(entry, variant)];
    if (view && !isTerminal(view))
      this.agent.notify('_pwr/download_cancel', { downloadId: view.downloadId });
  }

  /** Chooses a downloaded model for this workspace and closes the manager. */
  async use(modelRef: string): Promise<void> {
    await this.agent.selectModel(modelRef);
    this.close();
  }

  private listen(): void {
    if (this.listening) return;
    this.listening = true;
    this.agent.on('_pwr/download_progress', (params) => {
      const entry = Object.entries(this.downloads()).find(
        ([, view]) => view.downloadId === params.downloadId,
      );
      if (!entry || !params.state) return;
      const [key, view] = entry;
      // A terminal state is final, as the core's own state machine has it.
      if (view.state.state === 'completed') return;
      this.patch(key, { state: params.state, file: params.file ?? view.file });
    });
  }

  private markInstalled(repository: string, variantId: string, installed: boolean): void {
    this.results.update((entries) =>
      entries.map((entry) =>
        entry.repository !== repository
          ? entry
          : {
              ...entry,
              variants: entry.variants.map((variant) =>
                variant.id === variantId ? { ...variant, local: 'present', installed } : variant,
              ),
            },
      ),
    );
  }

  private put(key: string, view: DownloadView): void {
    this.downloads.update((all) => ({ ...all, [key]: view }));
  }

  private patch(key: string, change: Partial<DownloadView>): void {
    this.downloads.update((all) =>
      all[key] ? { ...all, [key]: { ...all[key], ...change } } : all,
    );
  }
}

export function isTerminal(view: DownloadView): boolean {
  return ['completed', 'failed', 'cancelled'].includes(view.state.state);
}

/** Filters as the core reads them: unset fields left out. */
function clean(filters: CatalogFilters): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(filters).filter(
      ([, value]) => value !== undefined && value !== null && value !== '',
    ),
  );
}
