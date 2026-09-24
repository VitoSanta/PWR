import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { bytes, fitTone, parameters, percent, tokens } from '../core/format';
import { CatalogEntry, CatalogVariant, DownloadView } from '../core/model';
import { ModelsStore } from '../core/models.store';
import { roveFocus } from '../core/ui';
import { Dialog } from './kit/dialog';
import { Icon } from './kit/icon';
import { Select, SelectOption } from './kit/select';
import { Tooltip } from './kit/tooltip';

/**
 * Find, judge and download models this machine can run. Every rating,
 * size and state comes from the core; this lays them out.
 */
@Component({
  selector: 'pa-model-manager',
  imports: [Dialog, Icon, Select, Tooltip],
  template: `
    @if (models.open()) {
      <pa-dialog
        size="lg"
        labelledBy="mm-title"
        describedBy="mm-description"
        (closed)="models.close()"
        animate.leave="is-leaving"
      >
        <header class="dialog-header mm-head">
          <span class="dialog-icon tone-accent"><pa-icon name="box" [size]="18" /></span>
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="mm-title">Model Manager</h2>
            <p class="dialog-description" id="mm-description">Open-weight models from Hugging Face, rated for this machine.</p>
          </div>
          <button class="icon-btn" (click)="models.close()" aria-label="Close Model Manager" paTooltip="Close">
            <pa-icon name="x" />
          </button>
        </header>

        <section class="machine" aria-label="This machine">
          @if (models.hardware(); as hw) {
            <dl class="machine-grid">
              <div class="machine-stat">
                <dt><pa-icon name="cpu" [size]="14" /> Machine</dt>
                <dd class="truncate">{{ hw.host.appleChip?.name ?? hw.host.cpu ?? 'unknown CPU' }} · {{ hw.host.architecture }}</dd>
              </div>
              <div class="machine-stat">
                <dt><pa-icon name="memory" [size]="14" /> {{ hw.host.memory.unified ? 'Unified memory' : 'Memory' }}</dt>
                <dd class="num">{{ b(hw.host.memory.totalBytes) }}</dd>
              </div>
              @for (gpu of discreteGpus(); track gpu.name) {
                <div class="machine-stat">
                  <dt><pa-icon name="monitor" [size]="14" /> GPU</dt>
                  <dd class="truncate">{{ gpu.name }} · {{ gpu.vramBytes ? b(gpu.vramBytes) : 'memory unknown' }}</dd>
                </div>
              }
              <div class="machine-stat">
                <dt><pa-icon name="hard-drive" [size]="14" /> Free disk</dt>
                <dd class="num">{{ hw.host.disk ? b(hw.host.disk.freeBytes) : 'unknown' }}</dd>
              </div>
              <div class="machine-stat">
                <dt><pa-icon name="info" [size]="14" /> OS</dt>
                <dd class="truncate">{{ hw.host.osName }} {{ hw.host.osVersion }}</dd>
              </div>
            </dl>
            <div class="engines" aria-label="Engines">
              @for (engine of hw.backends; track engine.id) {
                @if (engine.active) {
                <span class="badge" [class.badge-success]="engine.available && engine.active" [class.badge-outline]="!engine.active" [paTooltip]="engine.detail" tabindex="0">
                  <span class="dot" [class.dot-success]="engine.available" [class.dot-danger]="!engine.available" aria-hidden="true"></span>
                  {{ engine.label }}{{ engine.active ? ' · in use' : engine.available ? '' : ' · unavailable' }}
                </span>
                }
              }
            </div>
          } @else if (models.hardwareError()) {
            <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />This machine could not be read: {{ models.hardwareError() }}</p>
          } @else {
            <div class="machine-grid" aria-busy="true">
              @for (i of [1, 2, 3, 4]; track i) {
                <div class="machine-stat skeleton machine-skeleton"></div>
              }
            </div>
          }
        </section>

        <div class="mm-tabs tabs" role="tablist" aria-label="Models" (keydown)="tabKeys($event)">
          <button
            class="tab"
            role="tab"
            id="mm-tab-discover"
            aria-controls="mm-panel"
            [attr.aria-selected]="models.tab() === 'discover'"
            [attr.tabindex]="models.tab() === 'discover' ? 0 : -1"
            (click)="models.showTab('discover')"
          >
            <pa-icon name="search" [size]="14" /> Discover
          </button>
          <button
            class="tab"
            role="tab"
            id="mm-tab-local"
            aria-controls="mm-panel"
            [attr.aria-selected]="models.tab() === 'local'"
            [attr.tabindex]="models.tab() === 'local' ? 0 : -1"
            (click)="models.showTab('local')"
          >
            <pa-icon name="hard-drive" [size]="14" /> On this Mac
            @if (models.localRows().length) {
              <span class="count">{{ models.localRows().length }}</span>
            }
          </button>
        </div>

        <div class="mm-panel" role="tabpanel" id="mm-panel" [attr.aria-labelledby]="'mm-tab-' + models.tab()">
          @if (models.tab() === 'local') {
            <div class="mm-results">
              @if (models.localStatus() === 'loading' && !models.localRows().length) {
                  <p class="result-status"><span class="spinner spinner-sm" aria-hidden="true"></span> Reading the models folders…</p>
              } @else if (models.localStatus() === 'error' && !models.localRows().length) {
                  <div class="empty-state">
                    <span class="empty-state-icon"><pa-icon name="alert" /></span>
                    <p class="empty-state-title">The models folders could not be read</p>
                    <p class="empty-state-text">{{ models.localError() }}</p>
                  </div>
              } @else {
                @if (models.localStatus() === 'loading') {
                  <p class="result-status"><span class="spinner spinner-sm" aria-hidden="true"></span> Refreshing models…</p>
                }
                @for (model of models.localRows(); track model.format + model.modelRef) {
                    <div class="local-model" [class.local-model-downloading]="!!model.download">
                      <div class="local-main">
                        <strong class="truncate" [attr.title]="model.modelRef">{{ name(model.modelRef) }}</strong>
                        <span class="badge badge-outline">{{ model.format.toUpperCase() }}</span>
                        @if (model.download?.state?.state === 'cancelled') {
                          <span class="badge badge-warning">paused</span>
                        } @else if (model.download?.state?.state === 'failed') {
                          <span class="badge badge-danger">download failed</span>
                        } @else if (model.download && ['preparing', 'downloading', 'verifying'].includes(model.download.state.state)) {
                          <span class="badge badge-info">{{ downloadStatus(model.download) }}</span>
                        } @else if (model.partial) {
                          <span class="badge badge-warning">unfinished download</span>
                        }
                        @if (model.inUse) {
                          <span class="badge badge-success">in use</span>
                        }
                      </div>
                      <span class="t-meta num local-size">
                        {{ b(model.bytes) }}
                        @if (model.download?.totalBytes) { of {{ b(model.download.totalBytes) }} }
                        · {{ model.files }} file{{ model.files === 1 ? '' : 's' }}
                      </span>
                      @if (model.download) {
                        <div class="download-progress local-download-progress" [attr.title]="model.download.file ?? ''">
                          <div class="progress" role="progressbar" [attr.aria-valuenow]="progress(model.download)" aria-valuemin="0" aria-valuemax="100" [attr.aria-label]="'Downloading ' + name(model.modelRef)">
                            <span [style.width.%]="progress(model.download)"></span>
                          </div>
                          <small class="t-caption num">{{ phase(model.download) }}</small>
                        </div>
                      }
                      <div class="variant-action">
                        @if (model.download && ['preparing', 'downloading', 'verifying'].includes(model.download.state.state)) {
                          <button class="btn btn-sm" (click)="models.pause(model.modelRef, model.format)">Pause</button>
                        } @else if (model.partial && !model.inUse) {
                          <button class="btn btn-sm" (click)="models.resume(model.modelRef, model.format)">Resume</button>
                        }
                        @if (model.usable && !model.inUse) {
                          <button class="btn btn-sm" (click)="models.use(model.modelRef)" [disabled]="agent.turnActive()">Use</button>
                        }
                        <button
                          class="btn btn-sm btn-danger-quiet"
                          (click)="models.askDelete(model)"
                          [disabled]="model.inUse"
                          [paTooltip]="model.inUse ? 'Choose another model for this workspace first' : 'Delete from this Mac'"
                        >
                          <pa-icon name="trash" [size]="14" /> {{ model.partial ? 'Discard' : 'Delete' }}
                        </button>
                      </div>
                      <small class="local-path selectable">{{ model.path }}</small>
                    </div>
                } @empty {
                  @if (models.localStatus() === 'loading') {
                    <p class="result-status"><span class="spinner spinner-sm" aria-hidden="true"></span> Reading the models folders…</p>
                  } @else {
                    <div class="empty-state">
                      <span class="empty-state-icon"><pa-icon name="box" /></span>
                      <p class="empty-state-title">No models on this Mac yet</p>
                      <p class="empty-state-text">Find one in Discover.</p>
                      <button class="btn" (click)="models.showTab('discover')"><pa-icon name="search" [size]="16" /> Discover models</button>
                    </div>
                  }
                }
              }
            </div>
          } @else {
            <div class="mm-toolbar">
              <form class="mm-search" (submit)="$event.preventDefault(); models.searchNow()" role="search">
                <div class="input-group mm-search-field">
                  <pa-icon name="search" [size]="16" />
                  <input
                    #searchField
                    class="input"
                    type="search"
                    placeholder="Search models — qwen coder, gemma, llama…"
                    [value]="models.query()"
                    (input)="models.typeQuery($any($event.target).value)"
                    aria-label="Search models"
                    data-autofocus
                  />
                </div>
                <!-- GGUF (llama.cpp) is offered only where llama.cpp is the engine in
                     use; on a Mac the app runs MLX alone, so the choice is not shown. -->
                @if (models.activeBackend() === 'llama') {
                  <div class="segmented" role="radiogroup" aria-label="Format" (keydown)="formatKeys($event)">
                    <button type="button" role="radio" [attr.aria-checked]="models.format() === 'mlx'" [attr.tabindex]="models.format() === 'mlx' ? 0 : -1" (click)="models.setFormat('mlx')">MLX</button>
                    <button type="button" role="radio" [attr.aria-checked]="models.format() === 'gguf'" [attr.tabindex]="models.format() === 'gguf' ? 0 : -1" (click)="models.setFormat('gguf')">GGUF</button>
                  </div>
                }
                <button class="btn btn-primary" type="submit">Search</button>
              </form>

              <div class="mm-filters" aria-label="Filters">
                <label class="check-label">
                  <input
                    class="checkbox"
                    type="checkbox"
                    [checked]="models.filters().compatibleOnly"
                    (change)="models.setFilters({ compatibleOnly: $any($event.target).checked })"
                  />
                  Fits this machine
                </label>
                <span class="mm-filter-sep" aria-hidden="true"></span>
                <pa-select size="sm" ariaLabel="Parameters" [options]="sizeOptions" [value]="sizeValue()" (valueChange)="setSize($any($event))" />
                <pa-select size="sm" ariaLabel="Context length" [options]="contextOptions" [value]="models.filters().minContext" (valueChange)="models.setFilters({ minContext: $any($event) })" />
                <pa-select size="sm" ariaLabel="Download size" [options]="downloadOptions" [value]="models.filters().maxBytes" (valueChange)="models.setFilters({ maxBytes: $any($event) })" />
                <input
                  type="search"
                  class="input input-sm mm-filter-input"
                  aria-label="Quantization"
                  placeholder="Quantization (4-bit, Q4_K)"
                  [value]="models.filters().quantization ?? ''"
                  (input)="models.typeFilter({ quantization: $any($event.target).value || undefined })"
                />
                <input
                  type="search"
                  class="input input-sm mm-filter-input"
                  aria-label="Family"
                  placeholder="Family (qwen, gemma)"
                  [value]="models.filters().family ?? ''"
                  (input)="models.typeFilter({ family: $any($event.target).value || undefined })"
                />
              </div>

              @if (models.backend(); as engine) {
                @if (!engine.available) {
                  <p class="banner banner-warning"><pa-icon name="alert" [size]="16" />{{ engine.detail }}</p>
                } @else if (!engine.active) {
                  <p class="banner banner-warning">
                    <pa-icon name="info" [size]="16" />
                    <span>
                      These are {{ engine.format.toUpperCase() }} models for {{ engine.label }}. This app is running the
                      {{ models.activeBackend() }} engine: a download here can be chosen after starting the app with
                      <code class="path-token">PWR_BACKEND={{ engine.id }}</code>.
                    </span>
                  </p>
                }
              }

              <p class="result-status" aria-live="polite">
                @switch (models.status()) {
                  @case ('loading') {
                    <span class="spinner spinner-sm" aria-hidden="true"></span> Searching Hugging Face…
                  }
                  @case ('ready') {
                    <span>
                      {{ models.results().length }} model{{ models.results().length === 1 ? '' : 's' }}
                      · {{ (models.format() ?? '').toUpperCase() }}{{ models.query() ? ' · “' + models.query() + '”' : ''
                      }}{{ models.filters().compatibleOnly ? ' · fits this machine' : '' }}
                    </span>
                  }
                  @case ('error') {
                    Search failed
                  }
                }
              </p>
            </div>

            <div class="mm-results" [class.stale]="models.status() === 'loading' && models.results().length > 0">
              @switch (models.status() === 'loading' && models.results().length > 0 ? 'ready' : models.status()) {
                @case ('loading') {
                  @for (i of [1, 2, 3]; track i) {
                    <div class="skeleton model-skeleton" aria-hidden="true"></div>
                  }
                }
                @case ('error') {
                  <div class="empty-state">
                    <span class="empty-state-icon"><pa-icon name="globe" /></span>
                    <p class="empty-state-title">{{
                      models.error()?.kind === 'offline'
                        ? 'Hugging Face is not reachable'
                        : models.error()?.kind === 'rate_limited'
                          ? 'Too many requests'
                          : 'The search failed'
                    }}</p>
                    <p class="empty-state-text">{{ models.error()?.message }}</p>
                    <button class="btn" (click)="models.search()"><pa-icon name="refresh" [size]="16" /> Retry</button>
                  </div>
                }
                @default {
                  @for (entry of models.results(); track entry.repository) {
                    <article class="model-card" [attr.aria-labelledby]="'model-' + $index">
                      <header class="model-card-head">
                        <div class="model-card-title">
                          <h3 class="t-heading truncate" [id]="'model-' + $index" [attr.title]="entry.repository">{{ entry.name }}</h3>
                          <p class="t-meta truncate">
                            {{ entry.author ?? 'unknown author' }}
                            @if (entry.baseModel) {
                              · based on {{ entry.baseModel }}
                            }
                          </p>
                        </div>
                        <div class="model-card-stats">
                          @if (entry.downloads !== null) {
                            <span class="stat num" paTooltip="Downloads"><pa-icon name="download" [size]="12" />{{ entry.downloads.toLocaleString('en-US') }}</span>
                          }
                          @if (entry.likes !== null) {
                            <span class="stat num" paTooltip="Likes"><pa-icon name="heart" [size]="12" />{{ entry.likes.toLocaleString('en-US') }}</span>
                          }
                          <a class="icon-btn icon-btn-sm" [href]="entry.url" target="_blank" rel="noopener" aria-label="Open on Hugging Face" paTooltip="Open on Hugging Face">
                            <pa-icon name="external-link" [size]="14" />
                          </a>
                        </div>
                      </header>
                      <dl class="spec-line">
                        <div><dt>Parameters</dt><dd class="num" [attr.title]="entry.parametersSource ?? ''">{{ params(entry.parameters) }}</dd></div>
                        <div><dt>Context</dt><dd class="num" [attr.title]="entry.contextSource ?? ''">{{ entry.contextLength ? t(entry.contextLength) : 'unknown' }}</dd></div>
                        <div><dt>Architecture</dt><dd [attr.title]="entry.architectureSource ?? ''">{{ entry.architecture ?? 'unknown' }}</dd></div>
                        <div><dt>License</dt><dd>{{ entry.license ?? 'unknown' }}</dd></div>
                        <div><dt>Format</dt><dd>{{ entry.format.toUpperCase() }} · {{ entry.backend === 'mlx' ? 'MLX' : 'llama.cpp' }}</dd></div>
                        @if (entry.vision) {
                          <div><dt>Input</dt><dd><span class="badge badge-info"><pa-icon name="eye" [size]="12" /> text and images</span></dd></div>
                        }
                      </dl>
                      @for (note of entry.notes; track note) {
                        <p class="model-note"><pa-icon name="alert" [size]="14" />{{ note }}</p>
                      }

                      <div class="variants">
                        @for (variant of entry.variants; track variant.id) {
                          <div class="variant">
                            <div class="variant-main">
                              <strong class="variant-quant">{{ variant.quantization ?? 'unquantized' }}</strong>
                              @if (variant.quantizationSource === 'filename') {
                                <small class="muted" paTooltip="Read from the file name">from name</small>
                              }
                              <span class="t-meta num">{{ b(variant.bytes) }}{{ variant.files.length > 1 ? ' · ' + variant.files.length + ' files' : '' }}</span>
                              <button
                                class="badge"
                                [class]="'badge ' + fitBadge(variant.fit.level)"
                                (click)="toggleFit(entry, variant)"
                                [paTooltip]="variant.fit.explanation"
                                [attr.aria-expanded]="explained() === key(entry, variant)"
                              >
                                {{ variant.fit.label }}
                                <pa-icon name="info" [size]="12" />
                              </button>
                            </div>
                            <div class="variant-action">
                              @if (deletable(entry, variant)) {
                                <button
                                  class="btn btn-sm btn-danger-quiet"
                                  (click)="deleteVariant(variant)"
                                  [disabled]="variant.modelRef === agent.model()"
                                  [paTooltip]="variant.modelRef === agent.model() ? 'Choose another model first' : 'Delete from this Mac'"
                                >
                                  {{ variant.local === 'partial' ? 'Discard' : 'Delete' }}
                                </button>
                              }
                              @switch (action(entry, variant)) {
                                @case ('use') {
                                  <span class="variant-state text-success"><pa-icon name="check" [size]="14" /> Available</span>
                                  <button
                                    class="btn btn-sm btn-primary"
                                    (click)="models.use(variant.modelRef)"
                                    [disabled]="agent.turnActive() || variant.modelRef === agent.model()"
                                  >
                                    {{ variant.modelRef === agent.model() ? 'In use' : 'Use' }}
                                  </button>
                                }
                                @case ('running') {
                                  @if (download(entry, variant); as dl) {
                                    <div class="download-progress" [attr.title]="dl.file ?? ''">
                                      <div class="progress" role="progressbar" [attr.aria-valuenow]="progress(dl)" aria-valuemin="0" aria-valuemax="100" [attr.aria-label]="'Downloading ' + entry.name">
                                        <span [style.width.%]="progress(dl)"></span>
                                      </div>
                                      <small class="t-caption num">{{ phase(dl) }}</small>
                                    </div>
                                    <button class="btn btn-sm" (click)="models.pauseVariant(entry, variant)">Pause</button>
                                  }
                                }
                                @case ('done') {
                                  <span class="variant-state">{{ download(entry, variant)?.nextStep ?? 'Downloaded.' }}</span>
                                }
                                @case ('blocked') {
                                  <span class="variant-state">{{ variant.blocked }}</span>
                                }
                                @case ('incompatible') {
                                  <span class="variant-state">Cannot run here</span>
                                }
                                @default {
                                  @if (download(entry, variant); as dl) {
                                    @if (dl.state.state === 'failed') {
                                      <span class="variant-state text-danger" [paTooltip]="dl.state.message">{{ failure(dl) }}</span>
                                    }
                                    @if (dl.state.state === 'cancelled') {
                                      <span class="variant-state num">Paused · {{ b(dl.state.bytes) }} kept</span>
                                    }
                                  } @else if (variant.local === 'partial') {
                                    <span class="variant-state num">{{ b(variant.localBytes) }} downloaded</span>
                                  } @else if (variant.local === 'present') {
                                    <span class="variant-state">On disk</span>
                                  }
                                  <button
                                    [class]="variant.fit.level === 'not_recommended' ? 'btn btn-sm' : 'btn btn-sm btn-primary'"
                                    (click)="models.download(entry, variant)"
                                    [paTooltip]="variant.fit.level === 'not_recommended' ? 'Likely too large for this machine' : 'Download ' + b(variant.bytes)"
                                  >
                                    <pa-icon name="download" [size]="14" /> {{ verb(entry, variant) }}
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
                          <p class="fine">
                            No runnable {{ entry.format.toUpperCase() }} files in this repository{{ models.filters().compatibleOnly ? ' that fit this machine' : '' }}.
                          </p>
                        }
                      </div>
                    </article>
                  } @empty {
                    @if (models.status() === 'ready') {
                      <div class="empty-state">
                        <span class="empty-state-icon"><pa-icon name="search" /></span>
                        <p class="empty-state-title">{{ models.nextCursor() ? 'No matches on this page' : 'No models found' }}</p>
                        <p class="empty-state-text">
                          Nothing matched{{ models.filters().compatibleOnly ? ' that fits this machine' : '' }}.
                          {{ models.nextCursor() ? 'Load more to continue searching, or clear a filter.' : 'Try another search, or clear a filter.' }}
                        </p>
                      </div>
                    }
                  }
                  @if (models.nextCursor() && models.status() === 'ready') {
                    <div class="catalog-more">
                      @if (models.loadMoreError()) {
                        <span class="text-danger">{{ models.loadMoreError() }}</span>
                      }
                      <button class="btn" (click)="models.loadMore()" [disabled]="models.loadingMore()" [attr.aria-busy]="models.loadingMore()">
                        Load more models
                      </button>
                    </div>
                  }
                }
              }
            </div>
          }
        </div>
        <footer class="mm-foot">
          <pa-icon name="shield-check" [size]="14" />
          <span>
            Downloads go to <span class="mono">{{ models.modelsRoot() || 'the engine’s models folder' }}</span>, pinned to a commit
            and checked against the Hub's checksums. Fit is an estimate of whether a model loads with a useful context, not of
            its speed or quality.
          </span>
        </footer>
      </pa-dialog>
    }

    @if (models.pendingDelete(); as target) {
      <pa-dialog
        dialogRole="alertdialog"
        labelledBy="model-delete-title"
        describedBy="model-delete-message"
        [dismissible]="!models.deleting()"
        (closed)="models.cancelDelete()"
        animate.leave="is-leaving"
      >
        <div class="dialog-header">
          <span class="dialog-icon tone-danger"><pa-icon name="trash" [size]="18" /></span>
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="model-delete-title">Delete {{ name(target.modelRef) }}?</h2>
            <p class="dialog-description" id="model-delete-message">
              {{ b(target.bytes) }} will be permanently removed from this Mac and PWR will no longer list it. This cannot be
              undone; you can download it again later.
            </p>
          </div>
        </div>
        @if (target.path || models.deleteError()) {
          <div class="dialog-body">
            @if (target.path) {
              <p class="dialog-subject">{{ target.path }}</p>
            }
            @if (models.deleteError()) {
              <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ models.deleteError() }}</p>
            }
          </div>
        }
        <div class="dialog-footer">
          <button class="btn" (click)="models.cancelDelete()" [disabled]="models.deleting()" data-autofocus>Cancel</button>
          <button class="btn btn-danger" (click)="models.confirmDelete()" [disabled]="models.deleting()" [attr.aria-busy]="models.deleting()">
            Delete
          </button>
        </div>
      </pa-dialog>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ModelManager {
  protected readonly models = inject(ModelsStore);
  protected readonly agent = inject(AgentStore);

  protected readonly explained = signal<string | null>(null);
  protected readonly b = bytes;
  protected readonly t = tokens;
  protected readonly params = parameters;
  protected readonly tone = fitTone;
  protected readonly gib = 1024 ** 3;
  protected readonly sizeOptions: SelectOption<string>[] = [
    { value: '', label: 'Any size' },
    { value: '0-4', label: 'Up to 4B' },
    { value: '4-9', label: '4–9B' },
    { value: '9-16', label: '9–16B' },
    { value: '16-40', label: '16–40B' },
    { value: '40-', label: 'Over 40B' },
  ];
  protected readonly contextOptions: SelectOption<number | undefined>[] = [
    { value: undefined, label: 'Any context' },
    { value: 32768, label: '≥ 32k' },
    { value: 131072, label: '≥ 128k' },
  ];
  protected readonly downloadOptions: SelectOption<number | undefined>[] = [
    { value: undefined, label: 'Any download' },
    { value: 5 * this.gib, label: '≤ 5 GB' },
    { value: 10 * this.gib, label: '≤ 10 GB' },
    { value: 20 * this.gib, label: '≤ 20 GB' },
  ];

  protected sizeValue(): string {
    const { minParameters, maxParameters } = this.models.filters();
    if (minParameters == null && maxParameters == null) return '';
    return `${minParameters == null ? '0' : minParameters / 1e9}-${maxParameters == null ? '' : maxParameters / 1e9}`;
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
    if ('total' in state && 'bytes' in state) return percent(state.bytes, state.total);
    return 'bytes' in state && dl.totalBytes ? percent(state.bytes, dl.totalBytes) : 0;
  }

  protected downloadStatus(dl: DownloadView): string {
    switch (dl.state.state) {
      case 'preparing':
        return 'preparing';
      case 'downloading':
        return 'downloading';
      case 'verifying':
        return 'verifying';
      case 'cancelled':
        return 'paused';
      case 'failed':
        return 'failed';
      case 'completed':
        return 'downloaded';
    }
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
      case 'cancelled':
        return `Paused · ${bytes(state.bytes)} kept`;
      case 'failed':
        return `Failed · ${bytes(state.bytes)} kept`;
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

  protected fitBadge(level: string): string {
    return { ok: 'badge-success', fine: 'badge-info', warn: 'badge-warning', bad: 'badge-danger', muted: '' }[this.tone(level)];
  }

  protected tabKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=tab]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }

  protected formatKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }
}
