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
import { RunOutcome, Step, groupSteps } from '../core/trace';
import { Icon, IconName } from './kit/icon';
import { TraceCompact, TraceRaw, TraceSteps } from './trace';

/** One row of the conversation: the person's message, or the whole reply to it. */
type Item =
  | { type: 'user'; key: string; entry: Entry }
  | { type: 'turn'; key: string; entries: Entry[]; steps: Step[]; live: boolean; startedAt: number; endedAt: number }
  | { type: 'notice'; key: string; entry: Entry };

@Component({
  selector: 'pa-conversation',
  imports: [Icon, TraceCompact, TraceSteps, TraceRaw],
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
                  @switch (store.traceVisibility()) {
                    @case ('compact') {
                      <pa-trace-compact [entries]="item.entries" [live]="item.live" />
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
        @if (!store.turnActive() && store.runOutcome(); as outcome) {
          <div [class]="'outcome tone-' + outcome.tone" role="status">
            <pa-icon [name]="outcomeIcon(outcome.tone)" [size]="14" />
            <span>{{ outcome.text }}</span>
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
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller');
  private readonly now = signal(Date.now());
  private pinned = true;

  /**
   * Everything the model does between two messages of the person is one
   * turn -- reasoning, text, actions, retries in order, on one rail. The turn
   * keeps its entries; Compact, Detailed and Raw Trace each render them.
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
        turn = { type: 'turn', key: `turn-${entry.key}`, entries: [], steps: [], live: false, startedAt: entry.at, endedAt: entry.at };
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
    for (const item of items) if (item.type === 'turn') item.steps = groupSteps(item.entries);
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

  protected lastIsUser(): boolean {
    const items = this.items();
    return items[items.length - 1]?.type === 'user';
  }

  protected duration(item: { startedAt: number; endedAt: number; live: boolean }): string {
    const end = item.live ? this.now() : item.endedAt;
    const seconds = Math.max(0, Math.round((end - item.startedAt) / 1000));
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

/** A short visual type for an attachment. */
export function fileKind(path: string): { icon: IconName; label: string; tone: string } {
  const extension = /\.([a-z0-9]{1,6})$/i.exec(path)?.[1]?.toLowerCase();
  if (!extension) return { icon: 'folder', label: 'Folder', tone: 'folder' };
  if (extension === 'pdf') return { icon: 'file-text', label: 'PDF', tone: 'pdf' };
  if (['md', 'txt', 'rst'].includes(extension)) return { icon: 'file-text', label: extension.toUpperCase(), tone: 'text' };
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(extension)) return { icon: 'image', label: 'Image', tone: 'image' };
  return { icon: 'file-code', label: extension.toUpperCase(), tone: 'code' };
}
