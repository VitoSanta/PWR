import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { Entry } from '../core/model';
import { ModelsStore } from '../core/models.store';
import { NavigationService } from '../core/navigation';
import { PersonalStore } from '../core/personal.store';
import { ConfirmService, ToastService } from '../core/ui';
import { RunOutcome, Step, groupSteps } from '../core/trace';
import { WorkbenchStore } from '../core/workbench';
import { diffStats } from './diff';
import { Icon, IconName } from './kit/icon';
import { Popover } from './kit/popover';
import { Tooltip } from './kit/tooltip';
import { TraceCompact, TraceRaw, TraceSteps } from './trace';

/** What a turn changed: files, and lines added and removed. */
interface Edits {
  files: number;
  added: number;
  removed: number;
}

/** One row of the conversation: the person's message, or the whole reply to it. */
type Item =
  | { type: 'user'; key: string; entry: Entry }
  | { type: 'turn'; key: string; entries: Entry[]; steps: Step[]; live: boolean; startedAt: number; endedAt: number; modelName: string; actions: number; failed: number; edits: Edits }
  | { type: 'notice'; key: string; entry: Entry };

@Component({
  selector: 'pa-conversation',
  imports: [Icon, Popover, Tooltip, TraceCompact, TraceSteps, TraceRaw],
  template: `
    <section class="conversation" #scroller (wheel)="onWheel($event)" (scroll)="onScroll()">
      @if (store.timeline().length === 0) {
        <div class="welcome">
          <span class="welcome-where">{{ where() }}</span>
          <h2 class="welcome-title">{{ store.chatMode() ? 'What would you like to know?' : 'What are we building?' }}</h2>
          @if (store.model()) {
            <p class="welcome-text">
              @if (store.chatMode()) {
                Ask anything. PWR reads only what you attach, and cannot edit files or run commands.
              } @else {
                Describe the goal. PWR works in this folder and shows every step as it happens.
              }
            </p>
            <div class="welcome-context">
              @if (!store.chatMode()) {
                <span class="path-token" [attr.title]="store.workspace()">
                  <pa-icon name="folder" [size]="14" />
                  <span class="truncate">{{ store.workspace() || '…' }}</span>
                </span>
              }
              <span class="path-token">
                <pa-icon name="box" [size]="14" />
                <span class="truncate">{{ store.modelName() }}</span>
              </span>
            </div>
          } @else if (store.coreState() === 'ready') {
            <p class="welcome-text">
              Choose a model to start.
              {{ store.models().length ? 'Pick one of the models on this machine, or find another that fits it.' : 'There are no models on this machine yet — find one that fits it.' }}
            </p>
            <button class="btn btn-primary btn-lg" (click)="models.show()">
              <pa-icon name="box" [size]="16" /> Open Model Manager
            </button>
          } @else if (store.coreState() === 'starting') {
            <p class="welcome-text"><span class="spinner spinner-sm"></span> Starting the core…</p>
          }
        </div>
      }
      <div class="timeline">
        @if (store.timeline().length) {
          <header class="conversation-head">
            <span class="conversation-where truncate">{{ where() }} · {{ turns() }} turn{{ turns() === 1 ? '' : 's' }}</span>
            <h1 class="conversation-title">{{ nav.title() }}</h1>
          </header>
        }
        @for (item of items(); track item.key; let lastItem = $last) {
          @switch (item.type) {
            @case ('user') {
              <article class="message-user-group" aria-label="Your message">
                <span class="speaker">{{ person() }}</span>
                <div class="message-user" [class.is-long]="item.entry.text.length > 280">
                <div class="message-user-text selectable">{{ item.entry.text }}</div>
                @if (item.entry.attachments?.length) {
                  <div class="attachment-chips">
                    @for (path of item.entry.attachments; track path) {
                      <span class="attachment-chip" [attr.title]="path">
                        <pa-icon [name]="kindOf(path).icon" [size]="14" />
                        <span class="truncate">{{ name(path) }}</span>
                      </span>
                    }
                  </div>
                }
                </div>
                <div class="message-actions">
                  <button class="icon-btn icon-btn-sm" (click)="copy(item.entry.text)" aria-label="Copy message" paTooltip="Copy">
                    <pa-icon name="copy" [size]="14" />
                  </button>
                  @if (item.entry.turn !== undefined && !store.turnActive()) {
                    <button class="icon-btn icon-btn-sm" (click)="edit(item.entry)" [disabled]="store.rewinding()" aria-label="Edit message" paTooltip="Edit and send again">
                      <pa-icon name="pencil" [size]="14" />
                    </button>
                    <button
                      #rewindTrigger
                      class="icon-btn icon-btn-sm"
                      (click)="rewindMenu.set(rewindMenu() === item.key ? null : item.key)"
                      [disabled]="store.rewinding()"
                      aria-label="Rewind to here"
                      aria-haspopup="menu"
                      [attr.aria-expanded]="rewindMenu() === item.key"
                      paTooltip="Rewind to before this message"
                    >
                      <pa-icon name="history" [size]="14" />
                    </button>
                    @if (rewindMenu() === item.key) {
                      <pa-popover [anchor]="rewindTrigger" anchorAlign="end" width="300px" ariaLabel="Rewind" (closed)="rewindMenu.set(null)" animate.leave="anim-pop-out">
                        <div class="rewind-menu" role="menu">
                          <button class="rewind-option" role="menuitem" (click)="rewind(item.entry, true)">
                            <span class="settings-row-title">Conversation and files</span>
                            <span class="fine">Back to before this message; files PWR changed since are restored.</span>
                          </button>
                          <button class="rewind-option" role="menuitem" (click)="rewind(item.entry, false)">
                            <span class="settings-row-title">Conversation only</span>
                            <span class="fine">Back to before this message; files stay as they are.</span>
                          </button>
                        </div>
                      </pa-popover>
                    }
                  }
                </div>
              </article>
            }
            @case ('notice') {
              <article class="banner" [class.banner-danger]="item.entry.status === 'error'" role="note">
                <pa-icon [name]="item.entry.status === 'error' ? 'alert' : 'info'" [size]="16" />
                <span class="selectable"><strong>{{ item.entry.title }}</strong> {{ item.entry.text }}</span>
              </article>
            }
            @case ('turn') {
              @let folds = !item.live && item.actions > 0 && store.traceVisibility() === 'compact';
              <article class="turn" [class.live]="item.live" aria-label="PWR">
                <header class="turn-head">
                  <span class="speaker">PWR</span>
                  @if (folds) {
                    <button
                      class="turn-summary"
                      (click)="toggleWork(item.key)"
                      [attr.aria-expanded]="workOpen(item.key)"
                      [paTooltip]="item.modelName"
                    >
                      <pa-icon class="chevron" [class.open]="workOpen(item.key)" name="chevron-right" [size]="12" />
                      <span class="num">
                        @if (duration(item); as elapsed) { {{ elapsed }} · }{{ item.actions }} action{{ item.actions === 1 ? '' : 's' }}
                        @if (item.failed) { · <span class="text-danger">{{ item.failed }} failed</span> }
                      </span>
                    </button>
                  } @else {
                    <span class="turn-meta truncate">{{ item.modelName }}</span>
                    <span class="turn-meta num">· {{ item.live ? (store.chatMode() ? 'thinking' : 'working') : 'done' }}@if (duration(item); as elapsed) { · {{ elapsed }} }</span>
                  }
                </header>
                <div class="turn-body">
                  @if (item.entries.some(replayedTool) && (!folds || workOpen(item.key))) {
                    <div class="trace-replay-note" role="note">Restored action summary · detailed output, diffs and timings were not saved.</div>
                  }
                  @switch (store.traceVisibility()) {
                    @case ('compact') {
                      <pa-trace-compact [entries]="item.entries" [live]="item.live" [work]="!folds || workOpen(item.key)" />
                    }
                    @case ('detailed') {
                      <pa-trace-steps [steps]="item.steps" />
                    }
                    @case ('raw') {
                      <pa-trace-raw [entries]="item.entries" [startedAt]="item.startedAt" [endedAt]="item.endedAt" [live]="item.live" />
                    }
                  }
                  @if (item.live) {
                    <div class="step working" role="status">
                      <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
                      {{ workingLabel() }}
                    </div>
                  }
                </div>
                @if (!item.live) {
                  @let outcome = lastItem && !store.turnActive() ? store.runOutcome() : null;
                  @let text = answer(item.entries);
                  @if (item.edits.files || outcome || text) {
                    <div class="turn-chips">
                      @if (item.edits.files) {
                        <button class="note-chip" (click)="work.show('review')" paTooltip="Open the changes">
                          {{ item.edits.files }} file{{ item.edits.files === 1 ? '' : 's' }}
                          <span class="delta"><span class="add">+{{ item.edits.added }}</span><span class="del">−{{ item.edits.removed }}</span></span>
                        </button>
                      }
                      @if (outcome) {
                        <span [class]="'note-chip outcome tone-' + outcome.tone" role="status">
                          <span class="dot" aria-hidden="true"></span>
                          <span>{{ outcome.text }}</span>
                          @if (outcome.confinement) { <span class="t-meta">{{ outcome.confinement }}</span> }
                        </span>
                        @if (outcome.acceptanceChanges?.length) { <button class="btn btn-sm" (click)="store.reviewAcceptanceChanges()">Review acceptance changes</button> }
                        @if (outcome.action) {
                          <button class="btn btn-sm" (click)="store.continueRun()">
                            <pa-icon [name]="outcome.action === 'retry' ? 'refresh' : 'arrow-right'" [size]="14" />
                            {{ outcome.action === 'retry' ? 'Retry' : 'Continue' }}
                          </button>
                        }
                      }
                      @if (text) {
                        <span class="message-actions turn-actions">
                          <button class="icon-btn icon-btn-sm" (click)="copy(text)" aria-label="Copy answer" paTooltip="Copy answer">
                            <pa-icon name="copy" [size]="14" />
                          </button>
                        </span>
                      }
                    </div>
                  }
                }
              </article>
            }
          }
        }
        @if (store.turnActive() && lastIsUser()) {
          <article class="turn live" aria-label="PWR">
            <header class="turn-head">
              <span class="speaker">PWR</span>
              <span class="turn-meta truncate">{{ store.modelName() }} · {{ store.chatMode() ? 'thinking' : 'working' }} · {{ pendingDuration() }}</span>
            </header>
            <div class="turn-body">
              <div class="step working" role="status">
                <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
                {{ workingLabel() }}
              </div>
            </div>
          </article>
        }
        <!-- A run that ended with no reply of its own: its outcome on its own line. -->
        @if (!store.turnActive() && !lastIsTurn() && store.runOutcome(); as outcome) {
          <div [class]="'outcome tone-' + outcome.tone" role="status">
            <pa-icon [name]="outcomeIcon(outcome.tone)" [size]="14" />
            <span>{{ outcome.text }}</span>
            @if (outcome.confinement) { <span class="t-meta">{{ outcome.confinement }}</span> }
            @if (outcome.acceptanceChanges?.length) { <button class="btn btn-sm" (click)="store.reviewAcceptanceChanges()">Review acceptance changes</button> }
            @if (outcome.action) {
              <button class="btn btn-sm" (click)="store.continueRun()">
                <pa-icon [name]="outcome.action === 'retry' ? 'refresh' : 'arrow-right'" [size]="14" />
                {{ outcome.action === 'retry' ? 'Retry' : 'Continue' }}
              </button>
            }
          </div>
        }
      </div>
    </section>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Conversation {
  protected readonly store = inject(AgentStore);
  protected readonly models = inject(ModelsStore);
  protected readonly nav = inject(NavigationService);
  protected readonly work = inject(WorkbenchStore);
  private readonly personal = inject(PersonalStore);
  protected readonly replayedTool = (entry: Entry) => entry.kind === 'tool' && entry.replayed === true;
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller');
  private readonly now = signal(Date.now());
  private pinned = true;
  private lastScrollTop = 0;

  /**
   * Everything the model does between two messages of the person is one
   * turn -- reasoning, text, actions, retries in order, on one rail. The turn
   * keeps its entries, so the chosen view can present the same run at different detail levels.
   */
  protected readonly items = computed<Item[]>(() => {
    const items: Item[] = [];
    const entries = this.store.timeline();
    let modelName = 'model not saved';
    let userAt = 0;
    for (const entry of entries) {
      if (entry.kind === 'user') {
        modelName = entry.modelName ?? (entry.replayed ? 'model not saved' : this.store.modelName());
        userAt = entry.at;
        items.push({ type: 'user', key: entry.key, entry });
        continue;
      }
      if (entry.kind === 'notice' && entry.title !== 'Checkpoint') {
        items.push({ type: 'notice', key: entry.key, entry });
        continue;
      }
      let turn = items[items.length - 1];
      if (turn?.type !== 'turn') {
        turn = { type: 'turn', key: `turn-${entry.key}`, entries: [], steps: [], live: false, startedAt: userAt || entry.at, endedAt: entry.at, modelName, actions: 0, failed: 0, edits: { files: 0, added: 0, removed: 0 } };
        items.push(turn);
      }
      turn.endedAt = Math.max(turn.endedAt, entry.at);
      turn.entries.push(entry);
    }
    const lastTurn = [...items].reverse().find((item) => item.type === 'turn');
    if (lastTurn && lastTurn.type === 'turn') {
      // Working only when it is the newest thing in the conversation: a turn
      // followed by the person's next message is finished, whatever runs now.
      lastTurn.live = this.store.turnActive() && items[items.length - 1] === lastTurn;
    }
    for (const item of items) {
      if (item.type !== 'turn') continue;
      item.steps = groupSteps(item.entries);
      const tools = item.entries.filter((entry) => entry.kind === 'tool');
      item.actions = tools.length;
      item.failed = tools.filter((entry) => entry.status === 'failed').length;
      item.edits = edits(tools);
    }
    return items;
  });

  /** Where the conversation is: "projects / pwr-website", or Chat. */
  protected readonly where = computed(() => {
    if (this.store.chatMode()) return 'Chat · no workspace';
    const parts = this.store.workspace().split(/[\\/]/).filter(Boolean);
    return parts.slice(-2).join(' / ') || 'No folder open';
  });

  protected readonly turns = computed(() => this.store.timeline().filter((entry) => entry.kind === 'user').length);

  /** Who writes: the name in Settings → Profile. */
  protected readonly person = computed(() => this.personal.profile().name?.trim() || 'You');

  /** A finished turn's work, folded under its summary unless opened. */
  private readonly opened = signal<Record<string, boolean>>({});

  protected workOpen(key: string): boolean {
    return this.opened()[key] ?? false;
  }

  protected toggleWork(key: string): void {
    this.opened.update((state) => ({ ...state, [key]: !state[key] }));
  }

  protected readonly workingLabel = computed(() => {
    const quiet = Math.floor((this.now() - this.store.lastEventAt()) / 1000);
    const chat = this.store.chatMode();
    // The engine reads the whole prompt before its first word; on a cold cache
    // that is minutes, and it says how far along it is.
    const reading = this.store.prefill();
    if (reading && this.now() - reading.at < 60_000) {
      const percent = Math.min(99, Math.floor((reading.processed / reading.total) * 100));
      return `Reading the conversation · ${percent}% (${reading.processed.toLocaleString('en-US')} of ${reading.total.toLocaleString('en-US')} tokens)`;
    }
    if (quiet < 5) return chat ? 'Thinking' : 'Working';
    const minutes = Math.floor(quiet / 60);
    const seconds = String(quiet % 60).padStart(2, '0');
    return chat
      ? `Thinking · no new output for ${minutes}m ${seconds}s — reading the message and what is attached`
      : `Working · no new output for ${minutes}m ${seconds}s — checks, reading the prompt, or a long file being written`;
  });

  constructor() {
    // The clock only matters for a running turn's duration and quiet time.
    setInterval(() => {
      if (this.store.turnActive()) this.now.set(Date.now());
    }, 1000);
    // Follow the newest entry while the person is at the bottom; leave them
    // where they are when they have scrolled up to read. A trace view change
    // can replace a large subtree and change its height without a new entry.
    effect(() => {
      this.store.timeline();
      this.store.turnActive();
      this.store.traceVisibility();
      if (!this.pinned) return;
      requestAnimationFrame(() => {
        const element = this.scroller().nativeElement;
        element.scrollTop = element.scrollHeight;
      });
    });
  }

  protected onWheel(event: WheelEvent): void {
    if (event.deltaY < 0) this.pinned = false;
  }

  protected onScroll(): void {
    const element = this.scroller().nativeElement;
    const top = element.scrollTop;
    const nearBottom = element.scrollHeight - top - element.clientHeight < 120;
    // A phase can shrink when streaming content settles. That clamps the
    // scroll position and emits a scroll event near the bottom without any
    // user intent to follow. Re-pin only after a downward scroll.
    if (!nearBottom) this.pinned = false;
    else if (top > this.lastScrollTop + 1) this.pinned = true;
    this.lastScrollTop = top;
  }

  private readonly confirm = inject(ConfirmService);
  private readonly toast = inject(ToastService);
  protected readonly rewindMenu = signal<string | null>(null);

  /** A turn's answer: its last reply. */
  protected answer(entries: Entry[]): string {
    return [...entries].reverse().find((entry) => entry.kind === 'reply')?.text.trim() ?? '';
  }

  protected async copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      this.toast.show('Copied');
    } catch {
      this.toast.show('Could not copy', 'danger');
    }
  }

  /** Edit a sent message: back to before it, files included, and its text in the composer. */
  protected async edit(entry: Entry): Promise<void> {
    const ok = await this.confirm.ask({
      title: 'Edit this message?',
      message:
        'The conversation goes back to before it and the files PWR changed since are restored. ' +
        'What commands did (installs, files a script wrote) is not undone.',
      confirmLabel: 'Edit',
    });
    if (ok) await this.runRewind(entry, true, true);
  }

  protected async rewind(entry: Entry, restoreFiles: boolean): Promise<void> {
    this.rewindMenu.set(null);
    await this.runRewind(entry, restoreFiles, false);
  }

  private async runRewind(entry: Entry, restoreFiles: boolean, edit: boolean): Promise<void> {
    const { conflicts } = await this.store.rewind(entry, { restoreFiles, edit });
    if (!conflicts.length) return;
    const force = await this.confirm.ask({
      title: 'Some files changed after PWR wrote them',
      message: 'Restoring them discards those changes. Keep them by rewinding the conversation only.',
      subject: conflicts.join('\n'),
      subjectIsText: true,
      confirmLabel: 'Restore anyway',
      tone: 'danger',
    });
    if (force) await this.store.rewind(entry, { restoreFiles, edit, force: true });
  }

  protected lastIsUser(): boolean {
    const items = this.items();
    return items[items.length - 1]?.type === 'user';
  }

  protected lastIsTurn(): boolean {
    const items = this.items();
    return items[items.length - 1]?.type === 'turn';
  }

  protected duration(item: { entries: Entry[]; startedAt: number; endedAt: number; live: boolean }): string {
    if (item.entries.every((entry) => entry.replayed)) return '';
    const end = item.live ? this.now() : item.endedAt;
    return this.formatDuration(end - item.startedAt);
  }

  protected pendingDuration(): string {
    const last = this.store.timeline().at(-1);
    return last?.kind === 'user' ? this.formatDuration(this.now() - last.at) : '';
  }

  private formatDuration(milliseconds: number): string {
    const seconds = Math.max(0, Math.round(milliseconds / 1000));
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
  }

  protected outcomeIcon(tone: RunOutcome['tone']): IconName {
    return tone === 'done' ? 'check-circle' : tone === 'paused' ? 'history' : tone === 'stopped' ? 'stop' : 'alert';
  }

  protected name(path: string): string {
    return path.split('/').filter(Boolean).pop() ?? path;
  }

  protected kindOf(path: string) {
    return fileKind(path);
  }
}

/** The files a turn's tool calls edited, and their lines added and removed. */
function edits(tools: Entry[]): Edits {
  const paths = new Set<string>();
  let added = 0;
  let removed = 0;
  for (const tool of tools) {
    if (!tool.diff) continue;
    paths.add(tool.diff.path);
    const stats = diffStats(tool.diff.oldText, tool.diff.newText);
    added += stats.added;
    removed += stats.removed;
  }
  return { files: paths.size, added, removed };
}

/** A short visual type for an attachment. */
export function fileKind(path: string): { icon: IconName; label: string; tone: string } {
  const extension = /\.([a-z0-9]{1,6})$/i.exec(path)?.[1]?.toLowerCase();
  if (!extension) return { icon: 'folder', label: 'Folder', tone: 'folder' };
  if (extension === 'pdf') return { icon: 'file-text', label: 'PDF', tone: 'pdf' };
  if (['md', 'txt', 'rst'].includes(extension)) return { icon: 'file-text', label: extension.toUpperCase(), tone: 'text' };
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(extension)) return { icon: 'image', label: 'Image', tone: 'image' };
  return { icon: 'file-code', label: extension.toUpperCase(), tone: 'code' };
}
