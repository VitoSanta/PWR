import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { tokens } from '../core/format';
import { ModelsStore } from '../core/models.store';
import { RunMetrics, duration, rate, runMetrics } from '../core/trace';
import { Icon } from './kit/icon';
import { Popover } from './kit/popover';
import { Tooltip } from './kit/tooltip';

interface Figure {
  label: string;
  value: string;
  note?: string;
}

/**
 * Output speed of the last generation -- "24.3 tok/s" -- beside the context
 * meter, and, when clicked, the run's figures. Only what the core reported:
 * a figure nothing measured is left out rather than guessed.
 */
@Component({
  selector: 'pa-run-metrics',
  imports: [Icon, Popover, Tooltip],
  template: `
    <button
      #trigger
      class="topbar-chip run-metrics"
      (click)="toggle()"
      [attr.aria-expanded]="open()"
      aria-haspopup="dialog"
      [attr.aria-label]="speed() ? 'Output speed ' + speed() + ' tokens per second' : 'Runtime metrics'"
      paTooltip="Runtime metrics"
    >
      <pa-icon name="gauge" [size]="14" />
      @if (speed(); as speed) {
        <span class="num"><strong>{{ speed }}</strong><span class="meter-detail"> tok/s</span></span>
      } @else {
        <span class="num meter-detail">— tok/s</span>
      }
    </button>

    @if (open()) {
      <pa-popover [anchor]="trigger" anchorAlign="end" width="360px" ariaLabel="Runtime metrics" (closed)="open.set(false)" animate.leave="anim-pop-out">
        <div class="popover-head">
          <h2 class="popover-title">Runtime</h2>
          <button class="icon-btn icon-btn-sm" (click)="open.set(false)" aria-label="Close">
            <pa-icon name="x" [size]="16" />
          </button>
        </div>
        <div class="popover-body">
          @for (group of groups(); track group.title) {
            @if (group.figures.length) {
              <section class="popover-section" [attr.aria-label]="group.title">
                <h3 class="section-label">{{ group.title }}</h3>
                <dl class="facts metrics-facts">
                  @for (figure of group.figures; track figure.label) {
                    <dt [attr.title]="figure.note ?? null">{{ figure.label }}</dt>
                    <dd class="num">{{ figure.value }}</dd>
                  }
                </dl>
              </section>
            }
          }
          @if (missing().length) {
            <section class="popover-section">
              <p class="fine">Not available: {{ missing().join(', ') }}.</p>
            </section>
          }
        </div>
      </pa-popover>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RunMetricsChip {
  protected readonly store = inject(AgentStore);
  private readonly models = inject(ModelsStore);
  protected readonly open = signal(false);
  private readonly now = signal(Date.now());

  constructor() {
    setInterval(() => {
      if (this.open() && this.store.turnActive()) this.now.set(Date.now());
    }, 1000);
  }

  protected readonly metrics = computed<RunMetrics>(() => runMetrics(this.store.timeline(), this.now(), this.store.turnActive()));

  protected readonly speed = computed(() => {
    const value = this.metrics().generationTps;
    return value === null ? '' : rate(value);
  });

  protected readonly groups = computed(() => {
    const m = this.metrics();
    const usage = this.store.usage();
    const approx = m.tokenAccounting && m.tokenAccounting !== 'engine_tokenizer' && m.tokenAccounting !== 'server_usage' ? '~' : '';
    const figures = (list: (Figure | null)[]) => list.filter((figure): figure is Figure => figure !== null);
    const when = (value: number | null, show: (value: number) => string, label: string, note?: string): Figure | null =>
      value === null ? null : { label, value: show(value), note };
    const count = (value: number, label: string): Figure => ({ label, value: String(value) });
    const backend = this.models.hardware()?.backends.find((item) => item.active);
    return [
      {
        title: 'Last generation',
        figures: figures([
          when(m.promptTokens, (v) => approx + tokens(v), 'Input tokens'),
          when(m.generatedTokens, (v) => approx + tokens(v), 'Output tokens'),
          when(m.promptTokens !== null && m.generatedTokens !== null ? m.promptTokens + m.generatedTokens : null, (v) => approx + tokens(v), 'Total tokens'),
          when(m.reasoningTokens, (v) => tokens(v), 'of which reasoning'),
          when(m.promptEvalTps, (v) => `${rate(v)} tok/s`, 'Prompt eval', 'Prompt tokens over prefill time. A reused KV cache makes this higher than a cold prefill.'),
          when(m.generationTps, (v) => `${rate(v)} tok/s`, 'Generation'),
          when(m.firstTokenMs, duration, 'Time to first token', 'From the request to the first streamed chunk, measured by the core.'),
          when(m.generationMs, duration, 'Generation time'),
        ]),
      },
      {
        title: 'This run',
        figures: figures([
          when(m.runMs, duration, 'Total run time'),
          m.generations ? count(m.generations, 'Generations') : null,
          when(m.runInputTokens, (v) => approx + tokens(v), 'Input tokens (sum)', 'Every generation re-reads the prompt, so this counts it again each time.'),
          when(m.runOutputTokens, (v) => approx + tokens(v), 'Output tokens (sum)'),
          count(m.toolCalls, 'Tool calls'),
          count(m.commands, 'Commands'),
          count(m.filesRead, 'Files read'),
          count(m.filesEdited, 'Files edited'),
          { label: 'Retries', value: m.recovered ? `${m.retries} (${m.recovered} recovered)` : String(m.retries) },
          count(m.verifications, 'Verification runs'),
        ]),
      },
      {
        title: 'Runtime',
        figures: figures([
          this.store.model() ? { label: 'Model', value: this.store.modelName() } : null,
          backend ? { label: 'Engine', value: backend.label, note: backend.detail } : null,
          usage ? { label: 'Context', value: `${tokens(usage.used)} / ${tokens(usage.window)} · ${Math.round((usage.used / usage.window) * 100)}%` } : null,
        ]),
      },
    ];
  });

  protected readonly missing = computed(() => {
    const m = this.metrics();
    if (!m.generations && m.promptTokens === null) return [];
    const out: string[] = [];
    if (m.promptEvalTps === null) out.push('prompt eval speed');
    if (m.generationTps === null) out.push('generation speed');
    if (m.firstTokenMs === null) out.push('time to first token');
    out.push('peak memory', 'KV cache size');
    return out;
  });

  protected toggle(): void {
    const opening = !this.open();
    this.open.set(opening);
    this.now.set(Date.now());
    if (opening && !this.models.hardware()) void this.models.loadHardware();
  }
}
