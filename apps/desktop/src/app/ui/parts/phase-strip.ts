import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LoopStage, RUN_STATE_LABEL, RunStore } from '../../core/run';

/**
 * The agent's loop -- plan, inspect, implement, verify -- with the latest run
 * laid over it: which stages it reached, which it is in, what it did in each.
 * A strip across the top, or a column of margin notes.
 */
@Component({
  selector: 'pa-phase-strip',
  template: `
    <div class="phase-strip" [class.is-vertical]="vertical()" role="group" aria-label="The run's phases">
      <ol class="phase-stages">
        @for (stage of run.stages(); track stage.name) {
          <li [class]="'phase-stage is-' + stage.state" [attr.aria-current]="stage.state === 'active' ? 'step' : null">
            <span class="phase-mark" aria-hidden="true"></span>
            <span class="phase-stage-name">{{ stage.short }}</span>
            <span class="phase-stage-note">{{ note(stage) }}</span>
          </li>
        }
      </ol>
      @if (showState()) {
        <span [class]="'run-state is-' + run.state()" role="status">{{ stateLabel() }}</span>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PhaseStrip {
  protected readonly run = inject(RunStore);
  private readonly store = inject(AgentStore);

  readonly vertical = input(false);
  readonly showState = input(true);

  protected readonly stateLabel = computed(() =>
    this.store.chatMode() && this.run.state() === 'working' ? 'Thinking' : RUN_STATE_LABEL[this.run.state()],
  );

  /** What happened in a stage, in the fewest words: counts, never content. */
  protected note(stage: LoopStage): string {
    if (stage.state === 'active') return 'now';
    const s = stage.summary;
    if (!s) return '';
    const parts: string[] = [];
    if (s.read) parts.push(`${s.read} read`);
    if (s.edited) parts.push(`${s.edited} edited`);
    if (s.commands) parts.push(`${s.commands} run`);
    if (s.checks) parts.push(`${s.checks} check${s.checks === 1 ? '' : 's'}`);
    if (s.refused) parts.push(`${s.refused} refused`);
    if (!parts.length && s.reasoned) parts.push('reasoned');
    return parts.join(' · ');
  }
}
