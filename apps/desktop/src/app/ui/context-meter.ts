import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { fullness, percent, tokens, when } from '../core/format';
import { Icon } from './kit/icon';
import { Popover } from './kit/popover';
import { Select, SelectOption } from './kit/select';
import { Tooltip } from './kit/tooltip';

/**
 * How full the context window is -- "Context 42% · 54k / 128k" -- and, when
 * clicked, what fills it, when it compacts itself, and "Compact now".
 */
@Component({
  selector: 'pa-context-meter',
  imports: [Icon, Popover, Select, Tooltip],
  template: `
    <button
      #trigger
      class="topbar-chip meter"
      [class]="'topbar-chip meter level-' + level()"
      (click)="toggle()"
      [attr.aria-expanded]="open()"
      aria-haspopup="dialog"
      [attr.aria-label]="usage() ? 'Context ' + used() + '% used' : 'Context window'"
      paTooltip="Context window"
    >
      <svg class="meter-ring" viewBox="0 0 36 36" aria-hidden="true">
        <circle class="track" cx="18" cy="18" r="14" />
        <circle class="fill" cx="18" cy="18" r="14" [style.stroke-dasharray]="dash()" />
      </svg>
      @if (usage(); as usage) {
        <span class="num"><strong>{{ used() }}%</strong><span class="meter-detail"> · {{ t(usage.used) }} / {{ t(usage.window) }}</span></span>
      } @else {
        <span class="num">{{ t(window()) }}<span class="meter-detail"> window</span></span>
      }
    </button>

    @if (open()) {
      <pa-popover
        [anchor]="trigger"
        anchorAlign="end"
        width="380px"
        ariaLabel="Context"
        (closed)="open.set(false)"
        animate.leave="anim-pop-out"
      >
        <div class="popover-head">
          <h2 class="popover-title">Context</h2>
          <button class="icon-btn icon-btn-sm" (click)="open.set(false)" aria-label="Close">
            <pa-icon name="x" [size]="16" />
          </button>
        </div>

        @if (info(); as info) {
          <div class="popover-body">
            <section class="popover-section">
              <dl class="figures">
                <div>
                  <dt>Used</dt>
                  <dd class="figure-value num">{{ t(info.used) }}</dd>
                  <dd class="figure-note">{{ info.usedSource === 'engine' ? 'counted by engine' : 'estimated' }}</dd>
                </div>
                <div>
                  <dt>Window</dt>
                  <dd class="figure-value num">{{ t(info.window) }}</dd>
                  <dd class="figure-note">{{ store.context()?.setting ? 'your setting' : 'computed' }}</dd>
                </div>
                <div>
                  <dt>Remaining</dt>
                  <dd class="figure-value num">{{ t(max(0, info.window - info.used)) }}</dd>
                  <dd class="figure-note num">{{ 100 - pct(info.used, info.window) }}% free</dd>
                </div>
              </dl>
              <div class="bar" role="img" [attr.aria-label]="pct(info.used, info.window) + '% of the window used'">
                @for (part of parts(); track part.key) {
                  <span [class]="'seg k-' + part.key" [style.width.%]="part.share" [attr.title]="part.label + ': ~' + t(part.tokens)"></span>
                }
                <i class="threshold" [style.left.%]="info.autoCompact.thresholdPercent" title="Auto-compaction threshold"></i>
              </div>
              <ul class="legend">
                @for (part of parts(); track part.key) {
                  <li>
                    <span [class]="'swatch k-' + part.key" aria-hidden="true"></span>
                    <span class="truncate">{{ part.label }}</span>
                    <b class="num">~{{ t(part.tokens) }}</b>
                  </li>
                }
              </ul>
              <p class="fine">
                Composition is an estimate ({{ info.estimateBasis }}), ~{{ t(info.estimatedTokens) }} in all.
                @if (info.usedSource === 'engine') {
                  “Used” is the engine’s own count after the last reply.
                }
              </p>
            </section>

            <section class="popover-section" aria-label="Compaction">
              <div class="field-row">
                <span class="field-label" id="compact-label">Auto-compact at</span>
                <span class="compact-control">
                  <span class="t-meta num">≈ {{ t(info.autoCompact.thresholdTokens) }}</span>
                  <pa-select
                    size="sm"
                    labelledBy="compact-label"
                    [options]="thresholdOptions()"
                    [value]="info.autoCompact.thresholdPercent"
                    (valueChange)="setThreshold($any($event))"
                    [disabled]="store.turnActive()"
                  />
                </span>
              </div>
              <p class="fine">
                Between actions, a conversation past this share of its window summarises its older part.
                Repository files and indexes are never touched.
              </p>
              <dl class="facts">
                <dt>Last compaction</dt>
                <dd>
                  @if (info.lastCompaction; as last) {
                    {{ last.trigger === 'manual' ? 'By you' : 'Automatic' }} · {{ ago(last.at) }} ·
                    <span class="num">{{ t(last.tokensBefore) }} → {{ t(last.tokensAfter) }}</span>
                  } @else {
                    None yet
                  }
                </dd>
                @if (info.lastGeneration; as last) {
                  <dt paTooltip="Tokens of the last reply by phase. The reasoning itself is not kept.">Last reply</dt>
                  <dd class="figure-value num">
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
                  </dd>
                }
              </dl>
            </section>
          </div>

          <footer class="popover-foot">
            <button
              class="btn btn-block"
              (click)="store.compactNow()"
              [disabled]="store.turnActive() || store.compacting()"
              [attr.aria-busy]="store.compacting()"
            >
              <pa-icon name="refresh" [size]="16" /> Compact now
            </button>
            <p class="fine">
              Keeps the instructions, the task, files changed, paths, open errors and the last check result;
              folds older messages and tool output into one summary.
              @if (store.turnActive()) {
                Available when the turn ends.
              }
            </p>
          </footer>
        } @else {
          <div class="popover-section">
            @if (!store.sessionId()) {
              <p class="fine">
                The window is {{ t(window()) }} tokens{{ store.context()?.ceiling ? ', set by ' + store.context()?.ceiling : '' }}.
                Details appear once the conversation starts.
              </p>
            } @else if (loadError()) {
              <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ loadError() }}</p>
            } @else {
              <p class="fine loading-line"><span class="spinner spinner-sm" aria-hidden="true"></span> Reading the context…</p>
            }
          </div>
        }
      </pa-popover>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContextMeter {
  protected readonly store = inject(AgentStore);
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
    const circumference = 2 * Math.PI * 14;
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

  protected readonly thresholdOptions = computed<SelectOption<number>[]>(() => {
    const bounds = this.info()?.autoCompact.bounds;
    const out: SelectOption<number>[] = [];
    if (!bounds) return out;
    for (let value = bounds[0]; value <= bounds[1]; value += 5)
      out.push({ value, label: `${value}%`, hint: value === 75 ? 'default' : undefined });
    return out;
  });

  protected async toggle(): Promise<void> {
    const opening = !this.open();
    this.open.set(opening);
    if (opening) await this.reload();
  }

  protected async setThreshold(value: number): Promise<void> {
    try {
      await this.store.setAutoCompact(value);
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
}
