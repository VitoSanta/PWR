import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  computed,
  inject,
  signal,
} from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { fullness, percent, tokens, when } from '../core/format';

/**
 * How full the context window is -- "Context 42% · 54k / 128k" -- and, when
 * clicked, what fills it, when it compacts itself, and "Compact now".
 */
@Component({
  selector: 'pa-context-meter',
  template: `
    <button
      class="meter"
      [class]="'meter ' + level()"
      (click)="toggle()"
      [attr.aria-expanded]="open()"
      aria-haspopup="dialog"
      [title]="usage() ? 'Context window: click for details' : 'Context window'"
    >
      <svg viewBox="0 0 36 36" aria-hidden="true">
        <circle class="track" cx="18" cy="18" r="15" />
        <circle class="fill" cx="18" cy="18" r="15" [style.stroke-dasharray]="dash()" />
      </svg>
      @if (usage(); as usage) {
        <span
          >Context <strong>{{ used() }}%</strong> · {{ t(usage.used) }} /
          {{ t(usage.window) }}</span
        >
      } @else {
        <span>Context · {{ t(window()) }} window</span>
      }
    </button>

    @if (open()) {
      <div class="popover context-pop" role="dialog" aria-label="Context">
        <header>
          <h3>Context</h3>
          <button class="icon-button" (click)="open.set(false)" aria-label="Close">×</button>
        </header>

        @if (info(); as info) {
          <div class="context-figures">
            <div>
              <small>Used</small><strong>{{ t(info.used) }}</strong
              ><em>{{ info.usedSource === 'engine' ? 'counted by the engine' : 'estimated' }}</em>
            </div>
            <div>
              <small>Window</small><strong>{{ t(info.window) }}</strong
              ><em>{{ store.context()?.setting ? 'your setting' : 'computed' }}</em>
            </div>
            <div>
              <small>Remaining</small><strong>{{ t(max(0, info.window - info.used)) }}</strong
              ><em>{{ 100 - pct(info.used, info.window) }}% free</em>
            </div>
          </div>
          <div class="bar" [attr.aria-label]="pct(info.used, info.window) + '% used'">
            @for (part of parts(); track part.key) {
              <span
                [class]="'seg k-' + part.key"
                [style.width.%]="part.share"
                [title]="part.label + ': ~' + t(part.tokens)"
              ></span>
            }
            <i
              class="threshold"
              [style.left.%]="info.autoCompact.thresholdPercent"
              title="Auto-compaction threshold"
            ></i>
          </div>
          <ul class="legend">
            @for (part of parts(); track part.key) {
              <li>
                <span [class]="'swatch k-' + part.key"></span>{{ part.label
                }}<b>~{{ t(part.tokens) }}</b>
              </li>
            }
          </ul>
          <p class="fine">
            Composition is an estimate ({{ info.estimateBasis }}), ~{{ t(info.estimatedTokens) }} in
            all.
            @if (info.usedSource === 'engine') {
              "Used" is the engine's own count after the last reply.
            }
          </p>

          <section class="auto">
            <div class="row">
              <span>Auto-compact at</span>
              <select
                [value]="info.autoCompact.thresholdPercent"
                (change)="setThreshold($any($event.target).value)"
                [disabled]="store.turnActive()"
              >
                @for (option of thresholds(info.autoCompact.bounds); track option) {
                  <option
                    [value]="option"
                    [selected]="option === info.autoCompact.thresholdPercent"
                  >
                    {{ option }}%{{ option === 75 ? ' (default)' : '' }}
                  </option>
                }
              </select>
              <span class="muted">≈ {{ t(info.autoCompact.thresholdTokens) }}</span>
            </div>
            <p class="fine">
              Between actions, a conversation past this share of its window summarises its older
              part. Repository files and indexes are never touched.
            </p>
            <div class="row">
              <span>Last compaction</span>
              <span class="muted">
                @if (info.lastCompaction; as last) {
                  {{ last.trigger === 'manual' ? 'by you' : 'automatic' }} · {{ ago(last.at) }} ·
                  {{ t(last.tokensBefore) }} → {{ t(last.tokensAfter) }}
                } @else {
                  none yet
                }
              </span>
            </div>
            @if (info.lastGeneration; as last) {
              <div class="row">
                <span title="Tokens of the last reply by phase. The reasoning itself is not kept.">
                  Last reply
                </span>
                <span class="muted">
                  @if (last.reasoningTokens !== null) {
                    reasoning {{ approx(last) }}{{ t(last.reasoningTokens) }} ·
                  }
                  @if (last.answerTokens !== null) {
                    answer {{ approx(last) }}{{ t(last.answerTokens) }}
                  } @else {
                    {{ approx(last) }}{{ t(last.generatedTokens) }} generated
                  }
                  @if (last.effectiveTokens !== null) {
                    · budget {{ t(last.effectiveTokens) }}{{ last.clamped ? ' (lowered to fit)' : '' }}
                  }
                  @if (last.budgetReached) {
                    · closed at budget
                  }
                </span>
              </div>
            }
          </section>

          <footer>
            <button
              class="primary"
              (click)="store.compactNow()"
              [disabled]="store.turnActive() || store.compacting()"
            >
              {{ store.compacting() ? 'Compacting…' : 'Compact now' }}
            </button>
            <p class="fine">
              Keeps the instructions, the task, files changed, paths, open errors and the last check
              result; folds older messages and tool output into one summary.
              @if (store.turnActive()) {
                Available when the turn ends.
              }
            </p>
          </footer>
        } @else if (!store.sessionId()) {
          <p class="muted">
            The window is {{ t(window()) }} tokens{{
              store.context()?.ceiling ? ', set by ' + store.context()?.ceiling : ''
            }}. Details appear once the conversation starts.
          </p>
        } @else if (loadError()) {
          <p class="warn">{{ loadError() }}</p>
        } @else {
          <p class="muted">Reading the context…</p>
        }
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContextMeter {
  protected readonly store = inject(AgentStore);
  private readonly host = inject(ElementRef<HTMLElement>);
  protected readonly open = signal(false);
  protected readonly loadError = signal('');
  protected readonly t = tokens;

  /** "~" before a count that is an estimate rather than the engine's own. */
  protected approx(last: { tokenAccounting: string | null }): string {
    return last.tokenAccounting === 'engine_tokenizer' || last.tokenAccounting === 'server_usage'
      ? ''
      : '~';
  }
  protected readonly pct = percent;
  protected readonly max = Math.max;
  protected readonly ago = when;

  protected readonly info = this.store.contextInfo;
  /** The engine's count when there is one, else what the panel last read. */
  protected readonly usage = computed(() => {
    const reported = this.store.usage();
    if (reported) return reported;
    const info = this.info();
    return info && info.window > 0 ? { used: info.used, window: info.window } : null;
  });
  protected readonly window = computed(() => this.store.context()?.tokens ?? 0);
  protected readonly used = computed(() => {
    const usage = this.usage();
    return usage ? percent(usage.used, usage.window) : 0;
  });
  protected readonly level = computed(() =>
    fullness(this.used(), this.info()?.autoCompact.thresholdPercent ?? 75),
  );
  protected readonly dash = computed(() => {
    const circumference = 2 * Math.PI * 15;
    return `${(this.used() / 100) * circumference} ${circumference}`;
  });
  protected readonly parts = computed(() => {
    const info = this.info();
    if (!info) return [];
    const whole = Math.max(info.window, info.estimatedTokens, 1);
    const c = info.composition;
    return [
      { key: 'system', label: 'Instructions and harness rules', tokens: c.system },
      { key: 'conversation', label: 'Conversation', tokens: c.conversation },
      { key: 'repository', label: 'Repository and retrieved code', tokens: c.repository },
      { key: 'tools', label: 'Tool results', tokens: c.toolResults },
      { key: 'state', label: 'Task state', tokens: c.taskState },
      { key: 'memory', label: 'Compacted memory', tokens: c.compactedMemory },
    ]
      .filter((part) => part.tokens > 0)
      .map((part) => ({ ...part, share: (part.tokens / whole) * 100 }));
  });

  protected thresholds(bounds: [number, number]): number[] {
    const out: number[] = [];
    for (let value = bounds[0]; value <= bounds[1]; value += 5) out.push(value);
    return out;
  }

  protected async toggle(): Promise<void> {
    const opening = !this.open();
    this.open.set(opening);
    if (opening) await this.reload();
  }

  protected async setThreshold(value: string): Promise<void> {
    try {
      await this.store.setAutoCompact(Number(value));
    } catch (error) {
      this.loadError.set(String(error));
    }
  }

  private async reload(): Promise<void> {
    this.loadError.set('');
    try {
      await this.store.refreshContext();
    } catch (error) {
      this.loadError.set(String(error));
    }
  }

  @HostListener('document:mousedown', ['$event'])
  protected outside(event: MouseEvent): void {
    if (this.open() && !this.host.nativeElement.contains(event.target as Node))
      this.open.set(false);
  }

  @HostListener('document:keydown.escape')
  protected escape(): void {
    this.open.set(false);
  }
}
