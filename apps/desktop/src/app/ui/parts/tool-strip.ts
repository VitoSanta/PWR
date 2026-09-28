import { ChangeDetectionStrategy, Component, inject, output } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { UiStore, roveFocus } from '../../core/ui';
import { CardId, CardInfo, WorkbenchStore } from '../../core/workbench';
import { Icon, IconName } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';

/** Shorter names where the card's own would crowd a row. */
const SHORT: Partial<Record<CardId, string>> = { browser: 'Preview', plan: 'Checks' };

/**
 * The tools in one row: each opens its card, or closes it when it is
 * showing, and is lit while its card is open. Then the run's controls, the
 * commands and the settings. Opened from the Tools button, or by a right
 * click (a two-finger click) on an empty part of the window.
 */
@Component({
  selector: 'pa-tool-strip',
  imports: [Icon, Tooltip],
  template: `
    <div class="tool-strip" role="none" (keydown)="keys($event)">
      @for (card of work.available(); track card.id) {
        <button
          class="tool-strip-item"
          role="menuitemcheckbox"
          [attr.aria-checked]="showing(card)"
          (click)="work.toggle(card.id)"
          [paTooltip]="card.description"
          [paTooltipKeys]="card.keys ?? null"
        >
          <pa-icon [name]="icon(card)" [size]="18" />
          <span class="tool-strip-label">{{ label(card) }}</span>
        </button>
      }
      <span class="tool-strip-sep" aria-hidden="true"></span>
      @if (!store.chatMode()) {
        <button
          class="tool-strip-item"
          role="menuitem"
          [class.has-goal]="store.goalMode()"
          [class.has-auto]="store.permissionMode() === 'auto'"
          (click)="run.emit()"
          paTooltip="Goal mode and approvals"
        >
          <pa-icon name="zap" [size]="18" />
          <span class="tool-strip-label">Run</span>
        </button>
      }
      <button class="tool-strip-item" role="menuitem" (click)="ui.paletteOpen.set(true); done.emit()">
        <pa-icon name="command" [size]="18" />
        <span class="tool-strip-label">Commands</span>
      </button>
      <button class="tool-strip-item" role="menuitem" (click)="ui.settingsOpen.set(true); done.emit()">
        <pa-icon name="settings" [size]="18" />
        <span class="tool-strip-label">Settings</span>
      </button>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolStrip {
  protected readonly work = inject(WorkbenchStore);
  protected readonly store = inject(AgentStore);
  protected readonly ui = inject(UiStore);
  /** Something was chosen that takes the person elsewhere: the strip closes. */
  readonly done = output<void>();
  /** The run's controls were asked for. */
  readonly run = output<void>();

  protected showing(card: CardInfo): boolean {
    return this.work.panelVisible() && this.work.isOpen(card.id);
  }

  protected label(card: CardInfo): string {
    return SHORT[card.id] ?? card.label;
  }

  protected icon(card: CardInfo): IconName {
    return card.icon as IconName;
  }

  protected keys(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, '[role^=menuitem]', 'horizontal');
  }
}
