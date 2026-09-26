import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { Entry } from '../../core/model';
import { LoopStage, RUN_STATE_LABEL, RunStore } from '../../core/run';
import { ToolCategory, retryLabel, toolCategory, toolPath } from '../../core/trace';
import { WorkbenchStore } from '../../core/workbench';
import { diffStats } from '../diff';
import { Icon, IconName } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';

interface Card {
  key: string;
  icon: IconName;
  tool: string;
  subject: string;
  tone: 'live' | 'ok' | 'bad' | 'note';
  added?: number;
  removed?: number;
  path?: string;
}

const ICON: Record<ToolCategory, IconName> = {
  file_read: 'eye',
  search: 'search',
  file_edit: 'pencil',
  command: 'terminal',
  verification: 'shield-check',
  fetch: 'globe',
  malformed: 'alert',
  refused: 'x-circle',
  other: 'circle-dot',
};

/**
 * The latest run as a board: one column per stage of the agent's loop, one
 * card per thing it did there, in order. An edit's card opens its diff in
 * Review; a stage it has not reached is an empty column, so the board also
 * says what is still to come.
 */
@Component({
  selector: 'pa-run-board',
  imports: [Icon, Tooltip],
  template: `
    <section class="run-board" aria-label="The run">
      <header class="run-board-head">
        <div class="run-board-prompt">
          <span class="section-label">{{ store.chatMode() ? 'Question' : 'Task' }}</span>
          <p class="truncate-2">{{ run.prompt()?.text || 'No task yet: describe one in the conversation.' }}</p>
        </div>
        <span [class]="'run-state is-' + run.state()" role="status">{{ stateLabel() }}</span>
        <dl class="run-board-figures">
          <div><dt>Actions</dt><dd class="num">{{ run.actions().length }}</dd></div>
          <div><dt>Edited</dt><dd class="num">{{ edited() }}</dd></div>
          <div><dt>Checks</dt><dd class="num">{{ checks() }}</dd></div>
        </dl>
      </header>
      <div class="run-board-columns">
        @for (stage of run.stages(); track stage.name) {
          <section [class]="'run-column is-' + stage.state" [attr.aria-label]="stage.name">
            <header class="run-column-head">
              <span class="phase-mark" aria-hidden="true"></span>
              <span class="run-column-name">{{ stage.short }}</span>
              <span class="count num">{{ cards(stage).length }}</span>
            </header>
            <ol class="run-cards">
              @for (card of cards(stage); track card.key) {
                <li [class]="'run-card tone-' + card.tone">
                  @if (card.path && card.added !== undefined) {
                    <button class="run-card-body" (click)="work.show('review')" paTooltip="Open in Review">
                      <pa-icon [name]="card.icon" [size]="14" />
                      <span class="run-card-text">
                        <span class="run-card-tool">{{ card.tool }}</span>
                        <span class="run-card-subject truncate">{{ card.subject }}</span>
                      </span>
                      <span class="run-card-diff num"><span class="text-success">+{{ card.added }}</span> <span class="text-danger">−{{ card.removed }}</span></span>
                    </button>
                  } @else {
                    <div class="run-card-body">
                      <pa-icon [name]="card.icon" [size]="14" />
                      <span class="run-card-text">
                        <span class="run-card-tool">{{ card.tool }}</span>
                        <span class="run-card-subject truncate">{{ card.subject }}</span>
                      </span>
                      <span class="run-card-dot" aria-hidden="true"></span>
                    </div>
                  }
                </li>
              } @empty {
                <li class="run-card-empty">{{ stage.state === 'waiting' ? 'Not reached' : 'Reasoning only' }}</li>
              }
            </ol>
          </section>
        }
      </div>
    </section>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RunBoard {
  protected readonly run = inject(RunStore);
  protected readonly store = inject(AgentStore);
  protected readonly work = inject(WorkbenchStore);

  protected readonly stateLabel = computed(() => RUN_STATE_LABEL[this.run.state()]);
  protected readonly edited = computed(() => this.run.actions().filter((entry) => toolCategory(entry) === 'file_edit').length);
  protected readonly checks = computed(() => this.run.actions().filter((entry) => toolCategory(entry) === 'verification').length);

  /** A stage's cards: its actions and what interrupted them. Reasoning is a stage's, not a card. */
  protected cards(stage: LoopStage): Card[] {
    return stage.entries.flatMap((entry) => this.card(entry) ?? []);
  }

  private card(entry: Entry): Card | null {
    if (entry.kind === 'retry') {
      return { key: entry.key, icon: 'refresh', tool: 'Retry', subject: retryLabel(String(entry.data?.['cause'] ?? entry.title)), tone: 'note' };
    }
    if (entry.kind !== 'tool') return null;
    const category = toolCategory(entry);
    const [tool] = entry.title.split(/\s+/);
    const path = toolPath(entry) ?? '';
    const tone: Card['tone'] =
      entry.status === 'running' || entry.status === 'pending' ? 'live' : entry.status === 'failed' ? 'bad' : 'ok';
    const card: Card = {
      key: entry.key,
      icon: ICON[category],
      tool: tool || 'action',
      subject: category === 'command' || category === 'verification' ? entry.text : path || entry.text,
      tone,
    };
    if (entry.diff) {
      const { added, removed } = diffStats(entry.diff.oldText, entry.diff.newText);
      Object.assign(card, { added, removed, path: entry.diff.path });
    }
    return card;
  }
}
