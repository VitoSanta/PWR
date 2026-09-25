import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { Entry } from '../core/model';
import {
  PhaseName,
  Step,
  compactTurn,
  duration,
  groupSteps,
  isMalformed,
  recoveredLabel,
  retryLabel,
  summarizePhase,
  toolCategory,
} from '../core/trace';
import { Diff, diffStats } from './diff';
import { Follow } from './kit/follow';
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

const PHASE_ICONS: Record<PhaseName, IconName> = {
  'Inspecting workspace': 'search',
  Planning: 'target',
  Implementing: 'pencil',
  Verifying: 'shield-check',
  Fixing: 'refresh',
  Finalizing: 'check',
};

/** Open state that the person chose, else open only for the live group. */
abstract class Foldable {
  protected readonly store = inject(AgentStore);
  private readonly opened = signal<Record<string, boolean>>({});

  protected isOpen(key: string, fallback: boolean): boolean {
    return this.opened()[key] ?? (fallback && this.store.turnActive());
  }

  protected toggle(key: string, fallback: boolean): void {
    const open = this.isOpen(key, fallback);
    this.opened.update((state) => ({ ...state, [key]: !open }));
  }
}

/**
 * A phase's steps, as they happened -- the reasoning (a one-line summary,
 * opened for the whole), every tool call with its file, command or diff,
 * checks marked as checks, and each retry with the core's own reason.
 */
@Component({
  selector: 'pa-trace-steps',
  imports: [Markdown, Diff, Icon, Follow],
  template: `
    @for (step of steps(); track step.key) {
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
            @if (retried(step.entries); as retried) {
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
                @if (entry.kind === 'retry' || entry.kind === 'recovery') {
                  <li [class]="'action trace-event ' + entry.kind + ' ' + entry.status">
                    <div class="action-row">
                      <span class="tool-icon"><pa-icon [name]="entry.kind === 'recovery' ? 'check' : 'refresh'" [size]="14" /></span>
                      <span class="tool-title">{{ entry.kind === 'recovery' ? 'recovered' : 'retry' }}</span>
                      <span class="tool-detail selectable">{{ eventText(entry) }}</span>
                    </div>
                  </li>
                } @else {
                  <li [class]="'action ' + entry.status" [class.malformed]="malformed(entry)">
                    <button
                      class="action-row"
                      (click)="toggle(entry.key, false)"
                      [disabled]="!entry.diff"
                      [attr.aria-expanded]="entry.diff ? isOpen(entry.key, false) : null"
                    >
                      <span class="tool-icon"><pa-icon [name]="icon(entry)" [size]="14" /></span>
                      <span class="tool-title">{{ malformed(entry) ? 'malformed call' : verb(entry) }}</span>
                      <span class="tool-detail" [attr.title]="entry.text">{{ entry.text }}</span>
                      @if (category(entry) === 'verification') {
                        <span class="badge badge-info">check</span>
                      }
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
                {{ step.entry.status === 'live' ? 'Reasoning…' : 'Reasoning' }}
                <span class="muted num">{{ words(step.entry.text) }} words</span>
                <pa-icon class="chevron" [class.open]="isOpen(step.key, false)" name="chevron-right" [size]="14" />
              </button>
              @if (!isOpen(step.key, false)) {
                @if (step.entry.status === 'live') {
                  <!-- Anchored to the bottom: the newest lines are always whole, the
                       older ones fade out above instead of being cut mid-line. -->
                  <div class="thought-window"><pa-markdown class="thought-md" [text]="tail(step.entry.text)" [copyable]="false" /></div>
                } @else {
                  <p class="thought-summary">{{ gist(step.entry.text) }}</p>
                }
              }
              @if (isOpen(step.key, false)) {
                <div class="thought-body" paFollow><pa-markdown class="thought-md" [text]="step.entry.text" [copyable]="false" /></div>
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
          @case ('retry') {
            <div [class]="'step trace-line retry ' + step.entry.status">
              <pa-icon name="refresh" [size]="14" />
              <span class="selectable">{{ eventText(step.entry) }}</span>
            </div>
          }
          @case ('recovery') {
            <div class="step trace-line recovery"><pa-icon name="check" [size]="14" /> {{ eventText(step.entry) }}</div>
          }
          @case ('stop') {
            <div class="step trace-line stop selectable"><pa-icon name="alert" [size]="14" /> Stopped: {{ step.entry.text }}</div>
          }
        }
      }
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TraceSteps extends Foldable {
  readonly steps = input.required<Step[]>();
  protected readonly malformed = isMalformed;
  protected readonly category = toolCategory;

  protected busy(entries: Entry[]): boolean {
    return entries.some((entry) => entry.kind === 'tool' && (entry.status === 'running' || entry.status === 'pending'));
  }

  /** Refused by policy, not calls that failed to decode. */
  protected refused(entries: Entry[]): number {
    return entries.filter((entry) => entry.kind === 'tool' && entry.status === 'failed' && !isMalformed(entry)).length;
  }

  protected retried(entries: Entry[]): number {
    return entries.filter((entry) => entry.kind === 'retry').length;
  }

  protected summary(entries: Entry[]): string {
    const tools = entries.filter((entry) => entry.kind === 'tool' && !isMalformed(entry));
    const count = (kinds: string[]) => tools.filter((entry) => kinds.includes(entry.toolKind ?? 'other')).length;
    const parts: string[] = [];
    const read = count(['read', 'search']);
    const edited = count(['edit', 'delete', 'move']);
    const ran = count(['execute']);
    if (read) parts.push(`${read} read`);
    if (edited) parts.push(`${edited} edited`);
    if (ran) parts.push(`${ran} ran`);
    const other = tools.length - read - edited - ran;
    if (other > 0) parts.push(`${other} other`);
    return `${tools.length} action${tools.length === 1 ? '' : 's'}${parts.length ? ' · ' + parts.join(' · ') : ''}`;
  }

  /** The steps say the core's own reason beside the plain one. */
  protected eventText(entry: Entry): string {
    if (entry.kind === 'recovery') return recoveredLabel(Number(entry.data?.['retries'] ?? 1));
    const attempt = entry.data?.['attempt'];
    const limit = entry.data?.['limit'];
    const count = attempt && limit ? ` (${attempt}/${limit})` : '';
    return `${retryLabel(String(entry.data?.['cause'] ?? ''))}${count} — ${entry.text}`;
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
    return text.trimEnd().split(/\n{2,}/).slice(-3).join('\n\n');
  }

  /** A one-line summary: the reasoning's first sentence. */
  protected gist(text: string): string {
    const flat = text.replace(/[`*_#>]/g, '').replace(/\s+/g, ' ').trim();
    const sentence = /^(.{20,160}?[.!?])(\s|$)/.exec(flat)?.[1] ?? flat.slice(0, 160);
    return sentence.length < flat.length ? sentence.replace(/[.!?]?$/, '…') : sentence;
  }

  protected words(text: string): number {
    return text.split(/\s+/).filter(Boolean).length;
  }
}

/**
 * Compact: the run as phases of work -- inspecting, planning, implementing,
 * verifying, fixing, finalizing -- each with what it touched, and the result.
 * No reasoning text; retries in plain words. Each phase opens to its
 * steps.
 */
@Component({
  selector: 'pa-trace-compact',
  imports: [Markdown, Icon, TraceSteps],
  template: `
    @if (view().phases.length) {
      <ol class="step phases" [class.busy]="live()">
        @for (phase of view().phases; track phase.key; let last = $last) {
          <li class="phase" [class.running]="live() && last && !resultLive()" [class.failed]="phase.failed">
            <button
              class="phase-head"
              (click)="toggle(phase.key, false)"
              [disabled]="!phase.entries.length"
              [attr.aria-expanded]="phase.entries.length ? isOpen(phase.key, false) : null"
            >
              <span class="phase-icon">
                @if (live() && last) {
                  <span class="spinner spinner-sm"></span>
                } @else {
                  <pa-icon [name]="icon(phase.name)" [size]="14" />
                }
              </span>
              <span class="phase-name">{{ phase.name }}</span>
              <span class="phase-summary truncate">{{ phase.text }}</span>
              @if (phase.took) {
                <span class="phase-time num">{{ phase.took }}</span>
              }
              @if (phase.entries.length) {
                <pa-icon class="chevron" [class.open]="isOpen(phase.key, false)" name="chevron-right" [size]="14" />
              }
            </button>
            @if (phase.retry; as retry) {
              <div [class]="'phase-note tone-' + retry.tone" role="status">
                <pa-icon [name]="retry.tone === 'ok' ? 'check' : retry.tone === 'bad' ? 'alert' : 'refresh'" [size]="12" />
                {{ retry.text }}
              </div>
            }
            @if (isOpen(phase.key, false)) {
              <div class="phase-detail"><pa-trace-steps [steps]="phase.steps" /></div>
            }
          </li>
        }
      </ol>
    }
    @if (view().result; as result) {
      <div class="step text" [class.live]="result.status === 'live'">
        <pa-markdown [text]="result.text" />
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TraceCompact extends Foldable {
  readonly entries = input.required<Entry[]>();
  readonly live = input(false);

  protected readonly view = computed(() => {
    const turn = compactTurn(this.entries(), this.live());
    const stopped = this.entries().some((entry) => entry.kind === 'stop');
    return {
      result: turn.result,
      phases: turn.phases.map((phase) => {
        const summary = summarizePhase(phase);
        const parts: string[] = [];
        if (summary.read) parts.push(`${summary.read} read`);
        if (summary.edited) parts.push(`${summary.edited} edited`);
        if (summary.commands) parts.push(`${summary.commands} command${summary.commands === 1 ? '' : 's'}`);
        if (summary.checks) parts.push(`${summary.checks} check${summary.checks === 1 ? '' : 's'}`);
        if (summary.refused) parts.push(`${summary.refused} refused`);
        if (!parts.length && summary.reasoned) parts.push('thinking');
        const lastRetry = summary.retries[summary.retries.length - 1];
        let retry: { text: string; tone: 'ok' | 'warn' | 'bad' } | null = null;
        if (lastRetry?.status === 'running' && this.live()) {
          retry = { text: retryLabel(String(lastRetry.data?.['cause'] ?? '')), tone: 'warn' };
        } else if (lastRetry?.status === 'failed' || (lastRetry?.status === 'running' && stopped)) {
          retry = { text: 'Automatic retries exhausted', tone: 'bad' };
        } else if (summary.recovered) {
          retry = { text: recoveredLabel(summary.recovered), tone: 'ok' };
        }
        const span = phase.endedAt - phase.startedAt;
        return {
          ...phase,
          text: parts.join(' · '),
          took: span >= 1000 ? duration(span) : '',
          steps: groupSteps(phase.entries),
          retry,
          failed: retry?.tone === 'bad',
        };
      }),
    };
  });

  protected readonly resultLive = computed(() => this.view().result?.status === 'live');

  protected icon(name: PhaseName): IconName {
    return PHASE_ICONS[name];
  }
}
