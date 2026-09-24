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
import { Diff, diffStats } from './diff';
import { Icon, IconName } from './kit/icon';
import { Markdown } from './markdown';

const TOOL_ICONS: Record<string, IconName> = {
  read: 'eye',
  edit: 'pencil',
  delete: 'trash',
  move: 'move',
  search: 'search',
  execute: 'terminal',
  fetch: 'globe',
  think: 'sparkles',
  other: 'circle-dot',
};

/** A step inside one assistant turn. */
type Step =
  | { type: 'entry'; key: string; entry: Entry }
  | { type: 'actions'; key: string; entries: Entry[]; last: boolean };

/** One row of the conversation: the person's message, or the whole reply to it. */
type Item =
  | { type: 'user'; key: string; entry: Entry }
  | { type: 'turn'; key: string; steps: Step[]; live: boolean; startedAt: number; endedAt: number }
  | { type: 'notice'; key: string; entry: Entry };

@Component({
  selector: 'pa-conversation',
  imports: [Markdown, Diff, Icon],
  template: `
    <section class="conversation" #scroller (scroll)="onScroll()">
      @if (store.timeline().length === 0) {
        <div class="welcome">
          <img class="welcome-mark" src="/pwr-mark-96.png" alt="" width="44" height="44" />
          <h2 class="t-display">What are we building?</h2>
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
        @for (item of items(); track item.key) {
          @switch (item.type) {
            @case ('user') {
              <article class="message-user" aria-label="Your message">
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
              </article>
            }
            @case ('notice') {
              <article class="banner" [class.banner-danger]="item.entry.status === 'error'" role="note">
                <pa-icon [name]="item.entry.status === 'error' ? 'alert' : 'info'" [size]="16" />
                <span class="selectable"><strong>{{ item.entry.title }}</strong> {{ item.entry.text }}</span>
              </article>
            }
            @case ('turn') {
              <article class="turn" [class.live]="item.live" aria-label="PWR">
                <header class="turn-head">
                  <img class="turn-avatar" src="/pwr-mark-96.png" alt="" width="22" height="22" />
                  <strong>PWR</strong>
                  <span class="turn-meta truncate">{{ store.modelName() }}</span>
                  <span class="turn-meta num">· {{ item.live ? 'working' : 'done' }} · {{ duration(item) }}</span>
                </header>
                <div class="turn-body">
                  @for (step of item.steps; track step.key) {
                    @if (step.type === 'actions') {
                      <section class="step actions" [class.busy]="busy(step.entries)">
                        <button class="actions-head" (click)="toggle(step.key, step.last)" [attr.aria-expanded]="isOpen(step.key, step.last)">
                          <span class="actions-icon">
                            @if (busy(step.entries)) {
                              <span class="spinner spinner-sm"></span>
                            } @else if (refused(step.entries)) {
                              <pa-icon name="alert" [size]="16" />
                            } @else {
                              <pa-icon name="check-circle" [size]="16" />
                            }
                          </span>
                          <span class="actions-summary">{{ summary(step.entries) }}</span>
                          @if (malformed(step.entries); as retried) {
                            <span class="badge">{{ retried }} retried</span>
                          }
                          @if (refused(step.entries); as failed) {
                            <span class="badge badge-danger">{{ failed }} refused</span>
                          }
                          <pa-icon class="chevron" [class.open]="isOpen(step.key, step.last)" name="chevron-right" [size]="16" />
                        </button>
                        @if (isOpen(step.key, step.last)) {
                          <ul class="action-list">
                            @for (entry of step.entries; track entry.key) {
                              <li [class]="'action ' + entry.status" [class.malformed]="isMalformed(entry)">
                                <button
                                  class="action-row"
                                  (click)="toggle(entry.key, false)"
                                  [disabled]="!entry.diff"
                                  [attr.aria-expanded]="entry.diff ? isOpen(entry.key, false) : null"
                                >
                                  <span class="tool-icon"><pa-icon [name]="icon(entry)" [size]="14" /></span>
                                  <span class="tool-title">{{ isMalformed(entry) ? 'malformed call' : verb(entry) }}</span>
                                  <span class="tool-detail">{{ isMalformed(entry) ? explainMalformed(entry) : entry.text }}</span>
                                  @if (entry.diff) {
                                    <span class="delta">
                                      <span class="add">+{{ stats(entry).added }}</span>
                                      <span class="del">−{{ stats(entry).removed }}</span>
                                    </span>
                                  }
                                  <span [class]="'status-dot ' + entry.status" role="img" [attr.aria-label]="label(entry)" [attr.title]="label(entry)"></span>
                                </button>
                                @if (entry.diff && isOpen(entry.key, false)) {
                                  <pa-diff [diff]="entry.diff" />
                                }
                              </li>
                            }
                          </ul>
                        }
                      </section>
                    } @else {
                      @switch (step.entry.kind) {
                        @case ('thought') {
                          <div class="step thought" [class.live]="step.entry.status === 'live'">
                            <button class="thought-head" (click)="toggle(step.key, false)" [attr.aria-expanded]="isOpen(step.key, false)">
                              <pa-icon class="spark" name="sparkles" [size]="14" />
                              {{ step.entry.status === 'live' ? 'Thinking…' : 'Thought' }}
                              <span class="muted num">{{ words(step.entry.text) }} words</span>
                              <pa-icon class="chevron" [class.open]="isOpen(step.key, false)" name="chevron-right" [size]="14" />
                            </button>
                            @if (step.entry.status === 'live' && !isOpen(step.key, false)) {
                              <!-- Anchored to the bottom: the newest lines are always whole, the
                                   older ones fade out above instead of being cut mid-line. -->
                              <div class="thought-window"><pa-markdown class="thought-md" [text]="tail(step.entry.text)" [copyable]="false" /></div>
                            }
                            @if (isOpen(step.key, false)) {
                              <div class="thought-body"><pa-markdown class="thought-md" [text]="step.entry.text" [copyable]="false" /></div>
                            }
                          </div>
                        }
                        @case ('reply') {
                          <div class="step text" [class.live]="step.entry.status === 'live'">
                            <pa-markdown [text]="step.entry.text" />
                          </div>
                        }
                        @case ('notice') {
                          <div class="step checkpoint"><pa-icon name="refresh" [size]="14" /> {{ step.entry.text }}</div>
                        }
                      }
                    }
                  }
                  @if (item.live) {
                    <div class="step working" role="status">
                      <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
                      {{ workingLabel() }}
                    </div>
                  }
                </div>
              </article>
            }
          }
        }
        @if (store.turnActive() && lastIsUser()) {
          <article class="turn live" aria-label="PWR">
            <header class="turn-head">
              <img class="turn-avatar" src="/pwr-mark-96.png" alt="" width="22" height="22" />
              <strong>PWR</strong>
              <span class="turn-meta truncate">{{ store.modelName() }} · working</span>
            </header>
            <div class="turn-body">
              <div class="step working" role="status">
                <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
                {{ workingLabel() }}
              </div>
            </div>
          </article>
        }
        @if (!store.turnActive() && store.outcome()) {
          <div class="outcome" role="status"><pa-icon name="check-circle" [size]="14" /> {{ store.outcome() }}</div>
        }
      </div>
    </section>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Conversation {
  protected readonly store = inject(AgentStore);
  protected readonly models = inject(ModelsStore);
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller');
  private readonly opened = signal<Record<string, boolean>>({});
  private readonly now = signal(Date.now());
  private pinned = true;

  /**
   * Everything the model does between two messages of the person is one
   * turn -- reasoning, text and actions in order, on one rail -- and
   * consecutive actions inside it fold into one group.
   */
  protected readonly items = computed<Item[]>(() => {
    const items: Item[] = [];
    const entries = this.store.timeline();
    for (const entry of entries) {
      if (entry.kind === 'user') {
        items.push({ type: 'user', key: entry.key, entry });
        continue;
      }
      if (entry.kind === 'notice' && entry.title !== 'Checkpoint') {
        items.push({ type: 'notice', key: entry.key, entry });
        continue;
      }
      let turn = items[items.length - 1];
      if (turn?.type !== 'turn') {
        turn = { type: 'turn', key: `turn-${entry.key}`, steps: [], live: false, startedAt: entry.at, endedAt: entry.at };
        items.push(turn);
      }
      turn.endedAt = Math.max(turn.endedAt, entry.at);
      const previous = turn.steps[turn.steps.length - 1];
      if (entry.kind === 'tool') {
        if (previous?.type === 'actions') previous.entries.push(entry);
        else turn.steps.push({ type: 'actions', key: `group-${entry.key}`, entries: [entry], last: false });
      } else if (!(entry.kind === 'reply' && !entry.text.trim())) {
        // A streamed reply made only of whitespace is not a message.
        turn.steps.push({ type: 'entry', key: entry.key, entry });
      }
    }
    const lastTurn = [...items].reverse().find((item) => item.type === 'turn');
    if (lastTurn && lastTurn.type === 'turn') {
      // Working only when it is the newest thing in the conversation: a turn
      // followed by the person's next message is finished, whatever runs now.
      lastTurn.live = this.store.turnActive() && items[items.length - 1] === lastTurn;
      const lastGroup = [...lastTurn.steps].reverse().find((step) => step.type === 'actions');
      if (lastGroup && lastGroup.type === 'actions') lastGroup.last = true;
    }
    return items;
  });

  protected readonly workingLabel = computed(() => {
    const quiet = Math.floor((this.now() - this.store.lastEventAt()) / 1000);
    if (quiet < 5) return 'Working';
    const minutes = Math.floor(quiet / 60);
    const seconds = String(quiet % 60).padStart(2, '0');
    return `Working · no new output for ${minutes}m ${seconds}s — checks, reading the prompt, or a long file being written`;
  });

  constructor() {
    // The clock only matters for a running turn's duration and quiet time.
    setInterval(() => {
      if (this.store.turnActive()) this.now.set(Date.now());
    }, 1000);
    // Follow the newest entry while the person is at the bottom; leave them
    // where they are when they have scrolled up to read.
    effect(() => {
      this.store.timeline();
      this.store.turnActive();
      if (!this.pinned) return;
      requestAnimationFrame(() => {
        const element = this.scroller().nativeElement;
        element.scrollTo({ top: element.scrollHeight, behavior: 'smooth' });
      });
    });
  }

  protected onScroll(): void {
    const element = this.scroller().nativeElement;
    this.pinned = element.scrollHeight - element.scrollTop - element.clientHeight < 120;
  }

  /** Open state: what the person chose, else open only for the live group. */
  protected isOpen(key: string, fallback: boolean): boolean {
    return this.opened()[key] ?? (fallback && this.store.turnActive());
  }

  protected toggle(key: string, fallback: boolean): void {
    const open = this.isOpen(key, fallback);
    this.opened.update((state) => ({ ...state, [key]: !open }));
  }

  protected busy(entries: Entry[]): boolean {
    return entries.some((entry) => entry.status === 'running' || entry.status === 'pending');
  }

  /** Refused by policy, not calls that failed to decode. */
  protected refused(entries: Entry[]): number {
    return entries.filter((entry) => entry.status === 'failed' && !this.isMalformed(entry)).length;
  }

  protected malformed(entries: Entry[]): number {
    return entries.filter((entry) => this.isMalformed(entry)).length;
  }

  /** A call the core could not read: the model's form, retried, not a refusal. */
  protected isMalformed(entry: Entry): boolean {
    return /did not match its declared schema|could not be read|invalid type|missing field/i.test(entry.text);
  }

  protected explainMalformed(entry: Entry): string {
    const missing = /missing field `([^`]+)`/.exec(entry.text)?.[1];
    return missing ? `missing ${missing} — the model retries` : 'unreadable arguments — the model retries';
  }

  protected lastIsUser(): boolean {
    const items = this.items();
    return items[items.length - 1]?.type === 'user';
  }

  protected duration(item: { startedAt: number; endedAt: number; live: boolean }): string {
    const end = item.live ? this.now() : item.endedAt;
    const seconds = Math.max(0, Math.round((end - item.startedAt) / 1000));
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
  }

  protected summary(entries: Entry[]): string {
    const count = (kinds: string[]) => entries.filter((entry) => kinds.includes(entry.toolKind ?? 'other')).length;
    const parts: string[] = [];
    const read = count(['read', 'search']);
    const edited = count(['edit', 'delete', 'move']);
    const ran = count(['execute']);
    if (read) parts.push(`${read} read`);
    if (edited) parts.push(`${edited} edited`);
    if (ran) parts.push(`${ran} ran`);
    const other = entries.length - read - edited - ran - this.malformed(entries);
    if (other > 0) parts.push(`${other} other`);
    const real = entries.length - this.malformed(entries);
    return `${real} action${real === 1 ? '' : 's'}${parts.length ? ' · ' + parts.join(' · ') : ''}`;
  }

  protected verb(entry: Entry): string {
    return entry.title.split(' ')[0];
  }

  protected icon(entry: Entry): IconName {
    return TOOL_ICONS[entry.toolKind ?? 'other'] ?? 'circle-dot';
  }

  protected label(entry: Entry): string {
    return { pending: 'Planned', running: 'Running', done: 'Done', failed: 'Refused' }[entry.status as string] ?? entry.status;
  }

  protected stats(entry: Entry): { added: number; removed: number } {
    return entry.diff ? diffStats(entry.diff.oldText, entry.diff.newText) : { added: 0, removed: 0 };
  }

  /** The last paragraphs of the reasoning, enough to fill its window. */
  protected tail(text: string): string {
    const paragraphs = text.trimEnd().split(/\n{2,}/);
    return paragraphs.slice(-3).join('\n\n');
  }

  protected words(text: string): number {
    return text.split(/\s+/).filter(Boolean).length;
  }

  protected name(path: string): string {
    return path.split('/').filter(Boolean).pop() ?? path;
  }

  protected kindOf(path: string) {
    return fileKind(path);
  }
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
