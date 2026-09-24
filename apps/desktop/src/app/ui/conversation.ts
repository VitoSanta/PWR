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
import { Markdown } from './markdown';

const TOOL_ICONS: Record<string, string> = {
  read: '◎',
  edit: '✎',
  delete: '⌫',
  move: '⇄',
  search: '⌕',
  execute: '▶',
  fetch: '⇣',
  think: '✦',
  other: '•',
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
  imports: [Markdown, Diff],
  template: `
    <section class="conversation" #scroller (scroll)="onScroll()">
      @if (store.timeline().length === 0) {
        <div class="empty">
          <div class="empty-orb"></div>
          <h1>What are we building?</h1>
          @if (store.model()) {
            <p>Describe the goal. PWR works in <code>{{ store.workspace() || '…' }}</code> with
              <strong>{{ store.modelName() }}</strong>, and shows every step as it happens.</p>
          } @else if (store.coreState() === 'ready') {
            <p>Choose a model to start. {{ store.models().length ? 'Pick one of the models on this machine, or find another' : 'There are no models on this machine yet — find one' }} that fits it.</p>
            <button class="primary" (click)="models.show()">Open the Model Manager</button>
          }
        </div>
      }
      <div class="timeline">
        @for (item of items(); track item.key) {
          @switch (item.type) {
            @case ('user') {
              <article class="bubble user">
                <div class="text">{{ item.entry.text }}</div>
                @if (item.entry.attachments?.length) {
                  <div class="chips">
                    @for (path of item.entry.attachments; track path) {
                      <span class="chip">{{ kindOf(path).glyph }} {{ name(path) }}</span>
                    }
                  </div>
                }
              </article>
            }
            @case ('notice') {
              <article class="notice" [class.error]="item.entry.status === 'error'">
                <strong>{{ item.entry.title }}</strong> {{ item.entry.text }}
              </article>
            }
            @case ('turn') {
              <article class="turn" [class.live]="item.live">
                <header class="turn-head">
                  <span class="avatar">p</span>
                  <strong>PWR</strong>
                  <span class="turn-meta">{{ store.modelName() }} · {{ item.live ? 'working' : 'done' }} · {{ duration(item) }}</span>
                </header>
                <div class="turn-body">
                  @for (step of item.steps; track step.key) {
                    @if (step.type === 'actions') {
                      <section class="step actions" [class.busy]="busy(step.entries)">
                        <button class="actions-head" (click)="toggle(step.key, step.last)">
                          <span class="actions-icon">
                            @if (busy(step.entries)) { <span class="spinner"></span> } @else { ⚙ }
                          </span>
                          <span class="actions-summary">{{ summary(step.entries) }}</span>
                          @if (malformed(step.entries); as retried) {
                            <span class="pill soft">{{ retried }} retried</span>
                          }
                          @if (refused(step.entries); as failed) {
                            <span class="pill bad">{{ failed }} refused</span>
                          }
                          <span class="chev">{{ isOpen(step.key, step.last) ? '▾' : '▸' }}</span>
                        </button>
                        @if (isOpen(step.key, step.last)) {
                          <ul class="action-list">
                            @for (entry of step.entries; track entry.key) {
                              <li [class]="'action ' + entry.status" [class.malformed]="isMalformed(entry)">
                                <button class="action-row" (click)="toggle(entry.key, false)" [disabled]="!entry.diff">
                                  <span class="tool-icon">{{ icon(entry) }}</span>
                                  <span class="tool-title">{{ isMalformed(entry) ? 'malformed call' : verb(entry) }}</span>
                                  <span class="tool-detail">{{ isMalformed(entry) ? explainMalformed(entry) : entry.text }}</span>
                                  @if (entry.diff) {
                                    <span class="delta">
                                      <span class="add">+{{ stats(entry).added }}</span>
                                      <span class="del">−{{ stats(entry).removed }}</span>
                                    </span>
                                  }
                                  <span [class]="'status-dot ' + entry.status" [title]="label(entry)"></span>
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
                            <button class="thought-head" (click)="toggle(step.key, false)">
                              <span class="spark">✦</span>
                              {{ step.entry.status === 'live' ? 'Thinking…' : 'Thought' }}
                              <span class="muted">{{ words(step.entry.text) }} words</span>
                              <span class="chev">{{ isOpen(step.key, false) ? '▾' : '▸' }}</span>
                            </button>
                            @if (step.entry.status === 'live' && !isOpen(step.key, false)) {
                              <!-- Anchored to the bottom: the newest lines are always whole, the
                                   older ones fade out above instead of being cut mid-line. -->
                              <div class="thought-window"><pa-markdown class="thought-md" [text]="tail(step.entry.text)" /></div>
                            }
                            @if (isOpen(step.key, false)) {
                              <div class="thought-body"><pa-markdown class="thought-md" [text]="step.entry.text" /></div>
                            }
                          </div>
                        }
                        @case ('reply') {
                          <div class="step text" [class.live]="step.entry.status === 'live'">
                            <pa-markdown [text]="step.entry.text" />
                          </div>
                        }
                        @case ('notice') {
                          <div class="step checkpoint">↻ {{ step.entry.text }}</div>
                        }
                      }
                    }
                  }
                  @if (item.live) {
                    <div class="step working">
                      <span class="dots"><i></i><i></i><i></i></span>
                      {{ workingLabel() }}
                    </div>
                  }
                </div>
              </article>
            }
          }
        }
        @if (store.turnActive() && lastIsUser()) {
          <article class="turn live">
            <header class="turn-head">
              <span class="avatar">p</span>
              <strong>PWR</strong>
              <span class="turn-meta">{{ store.modelName() }} · working</span>
            </header>
            <div class="turn-body">
              <div class="step working">
                <span class="dots"><i></i><i></i><i></i></span>
                {{ workingLabel() }}
              </div>
            </div>
          </article>
        }
        @if (!store.turnActive() && store.outcome()) {
          <div class="outcome">{{ store.outcome() }}</div>
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
    setInterval(() => this.now.set(Date.now()), 1000);
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

  protected icon(entry: Entry): string {
    return TOOL_ICONS[entry.toolKind ?? 'other'] ?? '•';
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

  protected kindOf(path: string): { glyph: string } {
    return fileKind(path);
  }
}

/** A short visual type for an attachment. */
export function fileKind(path: string): { glyph: string; label: string; tone: string } {
  const extension = /\.([a-z0-9]{1,6})$/i.exec(path)?.[1]?.toLowerCase();
  if (!extension) return { glyph: '▦', label: 'Folder', tone: 'folder' };
  if (extension === 'pdf') return { glyph: '◧', label: 'PDF', tone: 'pdf' };
  if (['md', 'txt', 'rst'].includes(extension)) return { glyph: '≡', label: extension.toUpperCase(), tone: 'text' };
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(extension)) return { glyph: '◩', label: 'Image', tone: 'image' };
  return { glyph: '‹›', label: extension.toUpperCase(), tone: 'code' };
}
