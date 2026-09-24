import { ChangeDetectionStrategy, Component, effect, ElementRef, HostListener, inject, signal, ViewChild } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { bytes, fitTone, parameters, percent, tokens } from '../core/format';
import { CatalogEntry, CatalogVariant, DownloadView } from '../core/model';
import { ModelsStore } from '../core/models.store';

/**
 * Find, judge and download models this machine can run. Every rating,
 * size and state comes from the core; this lays them out.
 */
@Component({
  selector: 'pa-model-manager',
  template: `
    @if (models.open()) {
      <div class="scrim" (mousedown)="$event.target === $event.currentTarget && models.close()">
        <div class="sheet" role="dialog" aria-modal="true" aria-label="Model Manager">
          <header class="sheet-head">
            <div>
              <h2>Model Manager</h2>
              <p class="muted">Open-weight models from Hugging Face, rated for this machine.</p>
            </div>
            <button class="icon-button" (click)="models.close()" aria-label="Close">×</button>
          </header>

          <section class="machine">
            @if (models.hardware(); as hw) {
              <div class="machine-facts">
                <span class="fact"
                  ><small>Machine</small
                  >{{ hw.host.appleChip?.name ?? hw.host.cpu ?? 'unknown CPU' }} ·
                  {{ hw.host.architecture }}</span
                >
                <span class="fact"
                  ><small>{{ hw.host.memory.unified ? 'Unified memory' : 'Memory' }}</small
                  >{{ b(hw.host.memory.totalBytes) }}</span
                >
                @for (gpu of discreteGpus(); track gpu.name) {
                  <span class="fact"
                    ><small>GPU</small>{{ gpu.name }} ·
                    {{ gpu.vramBytes ? b(gpu.vramBytes) : 'memory unknown' }}</span
                  >
                }
                <span class="fact"
                  ><small>Free disk (models)</small
                  >{{ hw.host.disk ? b(hw.host.disk.freeBytes) : 'unknown' }}</span
                >
                <span class="fact"
                  ><small>OS</small>{{ hw.host.osName }} {{ hw.host.osVersion }}</span
                >
              </div>
              <div class="engines">
                @for (engine of hw.backends; track engine.id) {
                  <span class="engine" [class.ok]="engine.available" [title]="engine.detail">
                    <i></i>{{ engine.label }}{{ engine.active ? ' · in use' : '' }}
                  </span>
                }
              </div>
            } @else if (models.hardwareError()) {
              <p class="warn">This machine could not be read: {{ models.hardwareError() }}</p>
            } @else {
              <p class="muted">Reading this machine…</p>
            }
          </section>

          <nav class="sheet-tabs" role="tablist">
            <button
              role="tab"
              [class.on]="models.tab() === 'discover'"
              (click)="models.showTab('discover')"
            >
              Discover
            </button>
            <button
              role="tab"
              [class.on]="models.tab() === 'local'"
              (click)="models.showTab('local')"
            >
              On this Mac{{ models.local().length ? ' (' + models.local().length + ')' : '' }}
            </button>
          </nav>

          @if (models.tab() === 'local') {
            <div class="results">
              @switch (models.localStatus()) {
                @case ('loading') {
                  <p class="result-status">
                    <span class="spinner"></span> Reading the models folders…
                  </p>
                }
                @case ('error') {
                  <div class="state">
                    <strong>The models folders could not be read</strong>
                    <p>{{ models.localError() }}</p>
                  </div>
                }
                @default {
                  @for (model of models.local(); track model.format + model.modelRef) {
                    <div class="variant local-model">
                      <div class="variant-main">
                        <strong [title]="model.modelRef">{{ name(model.modelRef) }}</strong>
                        <span class="tag">{{ model.format.toUpperCase() }}</span>
                        <span class="muted"
                          >{{ b(model.bytes) }} · {{ model.files }} file{{
                            model.files === 1 ? '' : 's'
                          }}</span
                        >
                        @if (model.partial) {
                          <span class="fit warn">unfinished download</span>
                        }
                        @if (model.inUse) {
                          <span class="fit ok">in use here</span>
                        }
                      </div>
                      <div class="variant-action">
                        @if (model.usable && !model.inUse) {
                          <button
                            class="ghost"
                            (click)="models.use(model.modelRef)"
                            [disabled]="agent.turnActive()"
                          >
                            Use
                          </button>
                        }
                        <button
                          class="danger"
                          (click)="models.askDelete(model)"
                          [disabled]="model.inUse"
                          [title]="
                            model.inUse
                              ? 'Choose another model for this workspace first'
                              : 'Delete from this Mac'
                          "
                        >
                          {{ model.partial ? 'Discard' : 'Delete' }}
                        </button>
                      </div>
                      <small class="muted local-path">{{ model.path }}</small>
                    </div>
                  } @empty {
                    <div class="state">
                      <strong>No models on this Mac yet</strong>
                      <p>Find one in Discover.</p>
                    </div>
                  }
                }
              }
            </div>
          } @else {
            <form class="search" (submit)="$event.preventDefault(); models.searchNow()">
              <input
                #searchField
                type="search"
                placeholder="Search models — qwen coder, gemma, llama…"
                [value]="models.query()"
                (input)="models.typeQuery($any($event.target).value)"
                aria-label="Search models"
              />
              <div class="segmented" role="radiogroup" aria-label="Format">
                <button
                  type="button"
                  [class.on]="models.format() === 'mlx'"
                  (click)="models.setFormat('mlx')"
                >
                  MLX
                </button>
                <button
                  type="button"
                  [class.on]="models.format() === 'gguf'"
                  (click)="models.setFormat('gguf')"
                >
                  GGUF
                </button>
              </div>
              <button class="primary" type="submit">Search</button>
            </form>

            <div class="filters">
              <label class="check"
                ><input
                  type="checkbox"
                  [checked]="models.filters().compatibleOnly"
                  (change)="models.setFilters({ compatibleOnly: $any($event.target).checked })"
                />
                Fits this machine</label
              >
              <div class="filter-menu">
                <button type="button" class="filter-trigger" aria-label="Parameters" aria-haspopup="true" [attr.aria-expanded]="openFilter() === 'size'" (click)="toggleFilter('size')">{{ sizeLabel() }} <span aria-hidden="true">⌄</span></button>
                @if (openFilter() === 'size') {
                  <div class="filter-options" role="group" aria-label="Parameters">
                    @for (option of sizeOptions; track option.value) {
                      <button type="button" [class.selected]="sizeValue() === option.value" [attr.aria-pressed]="sizeValue() === option.value" (click)="setSize(option.value); openFilter.set(null)">{{ option.label }}</button>
                    }
                  </div>
                }
              </div>
              <div class="filter-menu">
                <button type="button" class="filter-trigger" aria-label="Context length" aria-haspopup="true" [attr.aria-expanded]="openFilter() === 'context'" (click)="toggleFilter('context')">{{ contextLabel() }} <span aria-hidden="true">⌄</span></button>
                @if (openFilter() === 'context') {
                  <div class="filter-options" role="group" aria-label="Context length">
                    @for (option of contextOptions; track option.value) {
                      <button type="button" [class.selected]="models.filters().minContext === option.value" [attr.aria-pressed]="models.filters().minContext === option.value" (click)="models.setFilters({ minContext: option.value }); openFilter.set(null)">{{ option.label }}</button>
                    }
                  </div>
                }
              </div>
              <div class="filter-menu">
                <button type="button" class="filter-trigger" aria-label="Disk size" aria-haspopup="true" [attr.aria-expanded]="openFilter() === 'download'" (click)="toggleFilter('download')">{{ downloadLabel() }} <span aria-hidden="true">⌄</span></button>
                @if (openFilter() === 'download') {
                  <div class="filter-options" role="group" aria-label="Disk size">
                    @for (option of downloadOptions; track option.value) {
                      <button type="button" [class.selected]="models.filters().maxBytes === option.value" [attr.aria-pressed]="models.filters().maxBytes === option.value" (click)="models.setFilters({ maxBytes: option.value }); openFilter.set(null)">{{ option.label }}</button>
                    }
                  </div>
                }
              </div>
              <label class="filter-field" (pointerdown)="quantizationField.focus()">
                <input
                  #quantizationField
                  type="search"
                  class="small-input"
                  aria-label="Quantization"
                  placeholder="Quantization (4-bit, Q4_K)"
                  [value]="models.filters().quantization ?? ''"
                  (input)="
                    models.typeFilter({ quantization: $any($event.target).value || undefined })
                  "
                />
              </label>
              <label class="filter-field" (pointerdown)="familyField.focus()">
                <input
                  #familyField
                  type="search"
                  class="small-input"
                  aria-label="Family"
                  placeholder="Family (qwen, gemma)"
                  [value]="models.filters().family ?? ''"
                  (input)="models.typeFilter({ family: $any($event.target).value || undefined })"
                />
              </label>
            </div>

            @if (models.backend(); as engine) {
              @if (!engine.available) {
                <div class="banner warnish">{{ engine.detail }}</div>
              } @else if (!engine.active) {
                <div class="banner warnish">
                  These are {{ engine.format.toUpperCase() }} models for {{ engine.label }}. This
                  app is running the {{ models.activeBackend() }} engine: a download here can be
                  chosen after starting the app with <code>PWR_BACKEND={{ engine.id }}</code
                  >.
                </div>
              }
            }

            <p class="result-status" aria-live="polite">
              @switch (models.status()) {
                @case ('loading') {
                  <span class="spinner"></span> Searching Hugging Face…
                }
                @case ('ready') {
                  {{ models.results().length }} model{{
                    models.results().length === 1 ? '' : 's'
                  }}
                  · {{ (models.format() ?? '').toUpperCase()
                  }}{{ models.query() ? ' · “' + models.query() + '”' : ''
                  }}{{ models.filters().compatibleOnly ? ' · fits this machine' : '' }}
                }
                @case ('error') {
                  Search failed
                }
              }
            </p>
            <div
              class="results"
              [class.stale]="models.status() === 'loading' && models.results().length > 0"
            >
              @switch (
                models.status() === 'loading' && models.results().length > 0
                  ? 'ready'
                  : models.status()
              ) {
                @case ('loading') {
                  @for (i of [1, 2, 3]; track i) {
                    <div class="card skeleton"></div>
                  }
                }
                @case ('error') {
                  <div class="state">
                    <strong>{{
                      models.error()?.kind === 'offline'
                        ? 'Hugging Face is not reachable'
                        : models.error()?.kind === 'rate_limited'
                          ? 'Too many requests'
                          : 'The search failed'
                    }}</strong>
                    <p>{{ models.error()?.message }}</p>
                    <button class="ghost" (click)="models.search()">Retry</button>
                  </div>
                }
                @default {
                  @for (entry of models.results(); track entry.repository) {
                    <article class="card">
                      <header class="card-head">
                        <div>
                          <h3>{{ entry.name }}</h3>
                          <p class="muted">
                            {{ entry.author ?? 'unknown author' }}
                            @if (entry.baseModel) {
                              · based on {{ entry.baseModel }}
                            }
                          </p>
                        </div>
                        <a class="hub-link" [href]="entry.url" target="_blank" rel="noopener"
                          >Hugging Face ↗</a
                        >
                      </header>
                      <dl class="meta">
                        <div>
                          <dt>Parameters</dt>
                          <dd [title]="entry.parametersSource ?? ''">
                            {{ params(entry.parameters) }}
                          </dd>
                        </div>
                        <div>
                          <dt>Architecture</dt>
                          <dd [title]="entry.architectureSource ?? ''">
                            {{ entry.architecture ?? 'unknown' }}
                          </dd>
                        </div>
                        <div>
                          <dt>Context</dt>
                          <dd [title]="entry.contextSource ?? ''">
                            {{ entry.contextLength ? t(entry.contextLength) : 'unknown' }}
                          </dd>
                        </div>
                        <div>
                          <dt>License</dt>
                          <dd>{{ entry.license ?? 'unknown' }}</dd>
                        </div>
                        <div>
                          <dt>Format</dt>
                          <dd>
                            {{ entry.format.toUpperCase() }} ·
                            {{ entry.backend === 'mlx' ? 'MLX' : 'llama.cpp' }}
                          </dd>
                        </div>
                        @if (entry.downloads !== null) {
                          <div>
                            <dt>Downloads</dt>
                            <dd>{{ entry.downloads.toLocaleString('en-US') }}</dd>
                          </div>
                        }
                        @if (entry.likes !== null) {
                          <div>
                            <dt>Likes</dt>
                            <dd>{{ entry.likes.toLocaleString('en-US') }}</dd>
                          </div>
                        }
                        @if (entry.vision) {
                          <div>
                            <dt>Input</dt>
                            <dd>text and images</dd>
                          </div>
                        }
                      </dl>
                      @for (note of entry.notes; track note) {
                        <p class="note">{{ note }}</p>
                      }

                      <div class="variants">
                        @for (variant of entry.variants; track variant.id) {
                          <div class="variant">
                            <div class="variant-main">
                              <strong>{{ variant.quantization ?? 'unquantized' }}</strong>
                              @if (variant.quantizationSource === 'filename') {
                                <small class="muted" title="Read from the file name"
                                  >from name</small
                                >
                              }
                              <span class="muted"
                                >{{ b(variant.bytes)
                                }}{{
                                  variant.files.length > 1
                                    ? ' · ' + variant.files.length + ' files'
                                    : ''
                                }}</span
                              >
                              <button
                                class="fit"
                                [class]="'fit ' + tone(variant.fit.level)"
                                (click)="toggleFit(entry, variant)"
                                [title]="variant.fit.explanation"
                                [attr.aria-expanded]="explained() === key(entry, variant)"
                              >
                                {{ variant.fit.label }}
                              </button>
                            </div>
                            <div class="variant-action">
                              @if (deletable(entry, variant)) {
                                <button
                                  class="ghost danger-text"
                                  (click)="deleteVariant(variant)"
                                  [disabled]="variant.modelRef === agent.model()"
                                  [title]="
                                    variant.modelRef === agent.model()
                                      ? 'Choose another model first'
                                      : 'Delete from this Mac'
                                  "
                                >
                                  {{ variant.local === 'partial' ? 'Discard' : 'Delete' }}
                                </button>
                              }
                              @switch (action(entry, variant)) {
                                @case ('use') {
                                  <span class="ok-text">Available</span>
                                  <button
                                    class="primary"
                                    (click)="models.use(variant.modelRef)"
                                    [disabled]="
                                      agent.turnActive() || variant.modelRef === agent.model()
                                    "
                                  >
                                    {{ variant.modelRef === agent.model() ? 'In use' : 'Use' }}
                                  </button>
                                }
                                @case ('running') {
                                  @if (download(entry, variant); as dl) {
                                    <div class="progress" [title]="dl.file ?? ''">
                                      <div class="track">
                                        <span [style.width.%]="progress(dl)"></span>
                                      </div>
                                      <small>{{ phase(dl) }}</small>
                                    </div>
                                    <button class="ghost" (click)="models.cancel(entry, variant)">
                                      Cancel
                                    </button>
                                  }
                                }
                                @case ('done') {
                                  <span class="muted">{{
                                    download(entry, variant)?.nextStep ?? 'Downloaded.'
                                  }}</span>
                                }
                                @case ('blocked') {
                                  <span class="muted">{{ variant.blocked }}</span>
                                }
                                @case ('incompatible') {
                                  <span class="muted">Cannot run here</span>
                                }
                                @default {
                                  @if (download(entry, variant); as dl) {
                                    @if (dl.state.state === 'failed') {
                                      <span class="bad-text" [title]="dl.state.message">{{
                                        failure(dl)
                                      }}</span>
                                    }
                                    @if (dl.state.state === 'cancelled') {
                                      <span class="muted"
                                        >Paused · {{ b(dl.state.bytes) }} kept</span
                                      >
                                    }
                                  } @else if (variant.local === 'partial') {
                                    <span class="muted"
                                      >{{ b(variant.localBytes) }} downloaded</span
                                    >
                                  } @else if (variant.local === 'present') {
                                    <span class="muted">On disk</span>
                                  }
                                  <button
                                    [class]="
                                      variant.fit.level === 'not_recommended' ? 'ghost' : 'primary'
                                    "
                                    (click)="models.download(entry, variant)"
                                    [title]="
                                      variant.fit.level === 'not_recommended'
                                        ? 'Likely too large for this machine'
                                        : 'Download ' + b(variant.bytes)
                                    "
                                  >
                                    {{ verb(entry, variant) }}
                                  </button>
                                }
                              }
                            </div>
                            @if (explained() === key(entry, variant)) {
                              <div class="explain">
                                <p>{{ variant.fit.explanation }}</p>
                                <ul>
                                  @for (assumption of variant.fit.assumptions; track assumption) {
                                    <li>{{ assumption }}</li>
                                  }
                                </ul>
                              </div>
                            }
                          </div>
                        } @empty {
                          <p class="muted">
                            No runnable {{ entry.format.toUpperCase() }} files in this repository{{
                              models.filters().compatibleOnly ? ' that fit this machine' : ''
                            }}.
                          </p>
                        }
                      </div>
                    </article>
                  } @empty {
                    @if (models.status() === 'ready') {
                      <div class="state">
                        <strong>{{ models.nextCursor() ? 'No matches on this page' : 'No models found' }}</strong>
                        <p>
                          Nothing matched{{
                            models.filters().compatibleOnly ? ' that fits this machine' : ''
                          }}. {{ models.nextCursor() ? 'Load more to continue searching, or clear a filter.' : 'Try another search, or clear a filter.' }}
                        </p>
                      </div>
                    }
                  }
                }
              }
            </div>
            @if (models.nextCursor() && models.status() === 'ready') {
              <div class="catalog-more">
                @if (models.loadMoreError()) {
                  <span class="bad-text">{{ models.loadMoreError() }}</span>
                }
                <button class="ghost" (click)="models.loadMore()" [disabled]="models.loadingMore()">
                  {{ models.loadingMore() ? 'Loading…' : 'Load more models' }}
                </button>
              </div>
            }
          }
          <footer class="sheet-foot muted">
            Downloads go to {{ models.modelsRoot() || 'the engine’s models folder' }}, pinned to a
            commit and checked against the Hub's checksums. Fit is an estimate of whether a model
            loads with a useful context, not of its speed or quality.
          </footer>

          @if (models.pendingDelete(); as target) {
            <div class="confirm-scrim">
              <div class="confirm" role="alertdialog" aria-modal="true">
                <h3>Delete {{ name(target.modelRef) }}?</h3>
                <p>
                  {{ b(target.bytes) }} will be permanently removed from this Mac and PWR will no
                  longer list it. This cannot be undone; you can download it again later.
                </p>
                @if (target.path) {
                  <p class="muted local-path">{{ target.path }}</p>
                }
                @if (models.deleteError()) {
                  <p class="bad-text">{{ models.deleteError() }}</p>
                }
                <div class="choices">
                  <button
                    class="ghost"
                    (click)="models.cancelDelete()"
                    [disabled]="models.deleting()"
                  >
                    Cancel
                  </button>
                  <button
                    class="danger"
                    (click)="models.confirmDelete()"
                    [disabled]="models.deleting()"
                  >
                    {{ models.deleting() ? 'Deleting…' : 'Delete' }}
                  </button>
                </div>
              </div>
            </div>
          }
        </div>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ModelManager {
  protected readonly models = inject(ModelsStore);
  protected readonly agent = inject(AgentStore);
  @ViewChild('searchField') private searchField?: ElementRef<HTMLInputElement>;
  private readonly focusDiscover = effect(() => {
    if (this.models.open() && this.models.tab() === 'discover') {
      setTimeout(() => this.searchField?.nativeElement.focus(), 0);
    }
  });

  protected readonly explained = signal<string | null>(null);
  protected readonly b = bytes;
  protected readonly t = tokens;
  protected readonly params = parameters;
  protected readonly tone = fitTone;
  protected readonly gib = 1024 ** 3;
  protected readonly openFilter = signal<'size' | 'context' | 'download' | null>(null);
  protected readonly sizeOptions = [
    { value: '', label: 'Any size' },
    { value: '0-4', label: 'Up to 4B' },
    { value: '4-9', label: '4–9B' },
    { value: '9-16', label: '9–16B' },
    { value: '16-40', label: '16–40B' },
    { value: '40-', label: 'Over 40B' },
  ];
  protected readonly contextOptions = [
    { value: undefined, label: 'Any context' },
    { value: 32768, label: '≥ 32k' },
    { value: 131072, label: '≥ 128k' },
  ];
  protected readonly downloadOptions = [
    { value: undefined, label: 'Any download' },
    { value: 5 * this.gib, label: '≤ 5 GB' },
    { value: 10 * this.gib, label: '≤ 10 GB' },
    { value: 20 * this.gib, label: '≤ 20 GB' },
  ];

  protected toggleFilter(filter: 'size' | 'context' | 'download'): void {
    this.openFilter.update((current) => current === filter ? null : filter);
  }

  protected sizeValue(): string {
    const { minParameters, maxParameters } = this.models.filters();
    if (minParameters == null && maxParameters == null) return '';
    return `${minParameters == null ? '0' : minParameters / 1e9}-${maxParameters == null ? '' : maxParameters / 1e9}`;
  }

  protected sizeLabel(): string {
    return this.sizeOptions.find((option) => option.value === this.sizeValue())?.label ?? 'Any size';
  }

  protected contextLabel(): string {
    return this.contextOptions.find((option) => option.value === this.models.filters().minContext)?.label ?? 'Any context';
  }

  protected downloadLabel(): string {
    return this.downloadOptions.find((option) => option.value === this.models.filters().maxBytes)?.label ?? 'Any download';
  }

  @HostListener('document:pointerdown', ['$event'])
  protected closeFilterOnOutsideClick(event: PointerEvent): void {
    if (!(event.target as Element).closest('.filter-menu')) this.openFilter.set(null);
  }

  protected discreteGpus() {
    return this.models.hardware()?.host.gpus.filter((gpu) => !gpu.unifiedMemory) ?? [];
  }

  protected key(entry: CatalogEntry, variant: CatalogVariant): string {
    return this.models.key(entry, variant);
  }

  protected download(entry: CatalogEntry, variant: CatalogVariant): DownloadView | undefined {
    return this.models.downloads()[this.key(entry, variant)];
  }

  /** Which control a variant shows, from what the core reported. */
  protected action(
    entry: CatalogEntry,
    variant: CatalogVariant,
  ): 'use' | 'running' | 'done' | 'blocked' | 'incompatible' | 'download' {
    const dl = this.download(entry, variant);
    if (variant.installed) return 'use';
    if (dl && ['preparing', 'downloading', 'verifying'].includes(dl.state.state)) return 'running';
    if (dl?.state.state === 'completed') return 'done';
    if (variant.fit.level === 'incompatible') return 'incompatible';
    if (variant.blocked) return 'blocked';
    return 'download';
  }

  protected verb(entry: CatalogEntry, variant: CatalogVariant): string {
    const dl = this.download(entry, variant);
    if (dl?.state.state === 'failed') return 'Retry';
    if (dl?.state.state === 'cancelled' || variant.local === 'partial') return 'Resume';
    if (variant.local === 'present') return 'Verify';
    return variant.fit.level === 'not_recommended' ? 'Download anyway' : 'Download';
  }

  protected progress(dl: DownloadView): number {
    const state = dl.state;
    return 'total' in state && 'bytes' in state ? percent(state.bytes, state.total) : 0;
  }

  protected phase(dl: DownloadView): string {
    const state = dl.state;
    switch (state.state) {
      case 'preparing':
        return 'Checking the files and the disk…';
      case 'downloading':
        return `${bytes(state.bytes)} of ${bytes(state.total)}`;
      case 'verifying':
        return `Verifying checksums… ${bytes(state.bytes)} of ${bytes(state.total)}`;
      default:
        return '';
    }
  }

  protected failure(dl: DownloadView): string {
    if (dl.state.state !== 'failed') return '';
    switch (dl.state.kind) {
      case 'insufficient_disk':
        return 'Not enough free disk space';
      case 'conflict':
        return 'A different file is already there';
      case 'network':
        return 'Network error — the partial file is kept';
      case 'verification':
        return 'Checksum mismatch';
      case 'unverifiable':
        return 'Cannot be verified';
      default:
        return 'Download failed';
    }
  }

  /** The model's name without its publisher: `Qwen3-4B-4bit`. */
  protected name(modelRef: string): string {
    const parts = modelRef.split('/');
    return parts.length > 1 ? parts.slice(1).join('/') : modelRef;
  }

  /** On disk and not downloading right now: it can be deleted. */
  protected deletable(entry: CatalogEntry, variant: CatalogVariant): boolean {
    if (this.action(entry, variant) === 'running') return false;
    const done = this.download(entry, variant)?.state.state === 'completed';
    return variant.installed || variant.local !== 'missing' || done;
  }

  protected deleteVariant(variant: CatalogVariant): void {
    this.models.askDelete({
      modelRef: variant.modelRef,
      format: variant.format,
      bytes: variant.bytes,
    });
  }

  protected toggleFit(entry: CatalogEntry, variant: CatalogVariant): void {
    const key = this.key(entry, variant);
    this.explained.set(this.explained() === key ? null : key);
  }

  protected setSize(value: string): void {
    const [min, max] = value ? value.split('-') : ['', ''];
    this.models.setFilters({
      minParameters: min ? Number(min) * 1e9 : undefined,
      maxParameters: max ? Number(max) * 1e9 : undefined,
    });
  }

  @HostListener('document:keydown.escape')
  protected escape(): void {
    if (this.openFilter()) this.openFilter.set(null);
    else if (this.models.pendingDelete()) this.models.cancelDelete();
    else if (this.models.open()) this.models.close();
  }
}
