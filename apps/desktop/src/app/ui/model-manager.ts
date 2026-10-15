import { MatButtonModule } from '@angular/material/button';
import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { bytes, percent, tokens } from '../core/format';
import { DownloadView, LinePack, ModelTarget } from '../core/model';
import { ModelsStore, isTerminal } from '../core/models.store';
import { Dialog } from './kit/dialog';
import { Icon } from './kit/icon';
import { Tooltip } from './kit/tooltip';

/**
 * The Gemma 4 line: one pack per memory tier, each downloaded from a pinned
 * revision and checked file by file. Every size, state and context figure
 * comes from the core; this lays them out.
 */
@Component({
  selector: 'pa-model-manager',
  imports: [MatButtonModule, Dialog, Icon, Tooltip],
  template: `
    @if (models.open()) {
      <pa-dialog size="lg" labelledBy="mm-title" describedBy="mm-description" (closed)="models.close()" animate.leave="is-leaving">
        <header class="dialog-header mm-head">
          <span class="dialog-icon tone-accent"><pa-icon name="box" [size]="18" /></span>
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="mm-title">Models</h2>
            <p class="dialog-description" id="mm-description">
              PWR runs Google's Gemma 4 family. Native context depends on the model; available context also depends on this Mac's memory.
            </p>
          </div>
          <button matIconButton class="icon-btn" (click)="models.close()" aria-label="Close Models" paTooltip="Close">
            <pa-icon name="x" />
          </button>
        </header>

        <section class="machine" aria-label="This machine">
          <dl class="machine-grid">
            <div class="machine-stat">
              <dt><pa-icon name="memory" [size]="14" /> Unified memory</dt>
              <dd class="num">{{ models.memoryBytes() ? b(models.memoryBytes()!) : 'unknown' }}</dd>
            </div>
          </dl>
        </section>

        <div class="mm-panel mm-line" role="list" aria-label="Gemma 4 models">
          @if (models.status() === 'error') {
            <div class="empty-state">
              <span class="empty-state-icon"><pa-icon name="alert" /></span>
              <p class="empty-state-title">The models could not be read</p>
              <p class="empty-state-text">{{ models.error() }}</p>
              <button matButton="outlined" class="btn" (click)="models.load()">Try again</button>
            </div>
          } @else {
            @if (models.status() === 'loading' && !models.packs().length) {
              <p class="result-status"><span class="spinner spinner-sm" aria-hidden="true"></span> Reading the models folder…</p>
            }
            @for (pack of models.packs(); track pack.model) {
              <div class="local-model" role="listitem" [class.local-model-downloading]="active(pack)">
                <div class="local-main">
                  <strong class="truncate" [attr.title]="pack.model">{{ pack.label }}</strong>
                  <span class="badge badge-outline">{{ pack.tierMemoryGib }} GB+</span>
                  @if (pack.inUse) {
                    <span class="badge badge-success">in use</span>
                  }
                  @if (pack.fullContext) {
                    <span class="badge badge-success" paTooltip="This Mac holds the whole context for it">{{ t(pack.windowTokens!) }} context</span>
                  } @else if (pack.windowTokens !== null) {
                    <span
                      class="badge badge-warning"
                      [paTooltip]="'Needs about ' + b(pack.neededBytes) + ' for the full context; this Mac holds ' + t(pack.windowTokens) + ' tokens'"
                      >{{ t(pack.windowTokens) }} context here</span
                    >
                  }
                  @if (download(pack); as dl) {
                    <span class="badge" [class]="'badge ' + (dl.state.state === 'failed' ? 'badge-danger' : dl.state.state === 'cancelled' ? 'badge-warning' : 'badge-info')">{{ status(dl) }}</span>
                  } @else if (pack.partial) {
                    <span class="badge badge-warning">unfinished download</span>
                  }
                </div>
                <span class="t-meta num local-size">{{ b(pack.weightsBytes) }}</span>
                @if (download(pack); as dl) {
                  @if (!isDone(dl) || dl.state.state !== 'completed') {
                    <div class="download-progress local-download-progress" [attr.title]="dl.file ?? ''">
                      <div class="progress" role="progressbar" [attr.aria-valuenow]="progress(dl)" aria-valuemin="0" aria-valuemax="100" [attr.aria-label]="'Downloading ' + pack.label">
                        <span [style.width.%]="progress(dl)"></span>
                      </div>
                      <small class="t-caption num">{{ phase(dl) }}</small>
                    </div>
                  }
                }
                <div class="variant-action local-actions">
                  @if (active(pack)) {
                    <button matButton="outlined" class="btn btn-sm" (click)="models.pause(pack)">Pause</button>
                  } @else if (!pack.installed) {
                    <button matButton="filled" class="btn btn-sm btn-primary" (click)="models.download(pack)" [disabled]="!pack.artifact">
                      {{ pack.partial || download(pack)?.state?.state === 'cancelled' ? 'Resume' : 'Download' }}
                    </button>
                  } @else if (!pack.inUse) {
                    <button matButton="outlined" class="btn btn-sm" (click)="models.use(pack)" [disabled]="agent.turnActive()">Use</button>
                  } @else {
                    <span class="action-slot" aria-hidden="true"></span>
                  }
                  <span class="action-slot" aria-hidden="true"></span>
                  <button matButton
                    class="btn btn-sm btn-danger-quiet"
                    (click)="models.askDelete(pack)"
                    [disabled]="pack.inUse || active(pack) || (!pack.installed && !pack.partial)"
                    [paTooltip]="pack.inUse ? 'Choose another model for this workspace first' : 'Delete from this Mac'"
                  >
                    <pa-icon name="trash" [size]="14" /> {{ pack.partial ? 'Discard' : 'Delete' }}
                  </button>
                </div>
                <!-- Its assistant: Google's small drafter, downloaded on its own
                     and used only while the switch in the model's panel is on. -->
                @let helper = models.assistantOf(pack);
                <div class="local-assistant" role="group" [attr.aria-label]="helper.label">
                  <div class="local-main">
                    <pa-icon name="sparkles" [size]="14" />
                    <span class="truncate" [attr.title]="helper.model">Assistant</span>
                    <span class="badge badge-outline" paTooltip="Drafts a few tokens ahead for Gemma to check. Turn it on in the model's panel; not yet faster on every pack.">experimental</span>
                    @if (download(helper); as dl) {
                      <span [class]="'badge ' + (dl.state.state === 'failed' ? 'badge-danger' : dl.state.state === 'cancelled' ? 'badge-warning' : 'badge-info')">{{ status(dl) }}</span>
                    } @else if (pack.assistant.installed) {
                      <span class="badge badge-success">downloaded</span>
                    } @else if (pack.assistant.partial) {
                      <span class="badge badge-warning">unfinished download</span>
                    }
                  </div>
                  <span class="t-meta num local-size">{{ b(helper.weightsBytes) }}</span>
                  @if (download(helper); as dl) {
                    @if (!isDone(dl) || dl.state.state !== 'completed') {
                      <div class="download-progress local-download-progress" [attr.title]="dl.file ?? ''">
                        <div class="progress" role="progressbar" [attr.aria-valuenow]="progress(dl)" aria-valuemin="0" aria-valuemax="100" [attr.aria-label]="'Downloading ' + helper.label">
                          <span [style.width.%]="progress(dl)"></span>
                        </div>
                        <small class="t-caption num">{{ phase(dl) }}</small>
                      </div>
                    }
                  }
                  <div class="variant-action local-actions">
                    @if (active(helper)) {
                      <button matButton="outlined" class="btn btn-sm" (click)="models.pause(helper)">Pause</button>
                    } @else if (!pack.assistant.installed) {
                      <button matButton="outlined" class="btn btn-sm" (click)="models.download(helper)" [disabled]="!helper.artifact">
                        {{ pack.assistant.partial || download(helper)?.state?.state === 'cancelled' ? 'Resume' : 'Download' }}
                      </button>
                    } @else {
                      <span class="action-slot" aria-hidden="true"></span>
                    }
                    <span class="action-slot" aria-hidden="true"></span>
                    <button matButton
                      class="btn btn-sm btn-danger-quiet"
                      (click)="models.askDelete(helper)"
                      [disabled]="active(helper) || (!pack.assistant.installed && !pack.assistant.partial)"
                      paTooltip="Delete from this Mac"
                    >
                      <pa-icon name="trash" [size]="14" /> {{ pack.assistant.partial ? 'Discard' : 'Delete' }}
                    </button>
                  </div>
                </div>
              </div>
            }
          }
        </div>
        <footer class="mm-foot">
          <pa-icon name="shield-check" [size]="14" />
          <span>
            Downloads go to <span class="mono">{{ models.modelsRoot() || 'the models folder' }}</span>, pinned to a commit and checked
            file by file against the sizes and SHA-256 PWR ships with.
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
            <h2 class="dialog-title" id="model-delete-title">Delete {{ target.label }}?</h2>
            <p class="dialog-description" id="model-delete-message">
              {{ b(target.bytesOnDisk) }} will be removed from this Mac. You can download it again later.
            </p>
          </div>
        </div>
        @if (models.deleteError()) {
          <div class="dialog-body">
            <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ models.deleteError() }}</p>
          </div>
        }
        <div class="dialog-footer">
          <button matButton="outlined" class="btn" (click)="models.cancelDelete()" [disabled]="models.deleting()" data-autofocus>Cancel</button>
          <button matButton="filled" class="btn btn-danger" (click)="models.confirmDelete()" [disabled]="models.deleting()" [attr.aria-busy]="models.deleting()">
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
  protected readonly b = bytes;
  protected readonly t = tokens;
  protected readonly isDone = isTerminal;

  protected download(pack: LinePack | ModelTarget): DownloadView | undefined {
    return this.models.downloads()[pack.model];
  }

  protected active(pack: LinePack | ModelTarget): boolean {
    const dl = this.download(pack);
    return !!dl && !isTerminal(dl);
  }

  protected progress(dl: DownloadView): number {
    const state = dl.state;
    if ('total' in state && 'bytes' in state) return percent(state.bytes, state.total);
    return 'bytes' in state && dl.totalBytes ? percent(state.bytes, dl.totalBytes) : 0;
  }

  protected status(dl: DownloadView): string {
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
        return this.failure(dl);
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
        return `${this.failure(dl)} · ${bytes(state.bytes)} kept`;
      default:
        return '';
    }
  }

  private failure(dl: DownloadView): string {
    if (dl.state.state !== 'failed') return '';
    switch (dl.state.kind) {
      case 'insufficient_disk':
        return 'Not enough free disk space';
      case 'conflict':
        return 'A different file is already there';
      case 'network':
        return 'Network error';
      case 'verification':
        return 'Checksum mismatch';
      default:
        return 'Download failed';
    }
  }
}
