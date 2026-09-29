import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { SHORTCUTS, UiStore, shortcut } from '../../core/ui';
import { CardId, CardInfo, WorkbenchStore } from '../../core/workbench';
import { Icon, IconName } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';

/** Shorter names where the card's own would crowd a row. */
const SHORT: Partial<Record<CardId, string>> = { browser: 'Preview', plan: 'Checks' };

/**
 * The tools: each opens its card, or closes it when it is showing, and is
 * lit while its card is open; a choice closes them, so they never stay over
 * the card just opened. Then the run's controls, the commands and the
 * settings. From the Tools button they drop down as a list, with their
 * shortcuts; from a right click (a two-finger click) on an empty part of
 * the window they open as a row where it landed.
 */
@Component({
  selector: 'pa-tool-strip',
  imports: [Icon, Tooltip],
  template: `
    <div class="tool-strip" [class.is-vertical]="vertical()" role="none">
      @for (card of work.available(); track card.id) {
        <button
          class="tool-strip-item"
          role="menuitemcheckbox"
          [attr.aria-checked]="showing(card)"
          (click)="work.toggle(card.id); done.emit()"
          [paTooltip]="vertical() ? null : card.description"
          [paTooltipKeys]="vertical() ? null : (card.keys ?? null)"
        >
          <pa-icon [name]="icon(card)" [size]="vertical() ? 16 : 18" />
          <span class="tool-strip-label">{{ vertical() ? card.label : label(card) }}</span>
          @if (vertical() && card.keys) {
            <span class="kbd tool-strip-keys">{{ keys(card.keys) }}</span>
          }
        </button>
      }
      <span class="tool-strip-sep" aria-hidden="true"></span>
      @if (!store.chatMode()) {
        <button
          class="tool-strip-item"
          role="menuitem"
          [class.has-goal]="store.goalMode()"
          [class.has-auto]="store.permissionMode() === 'full'"
          (click)="run.emit()"
          [paTooltip]="vertical() ? null : 'Goal mode and approvals'"
        >
          <pa-icon name="zap" [size]="vertical() ? 16 : 18" />
          <span class="tool-strip-label">{{ vertical() ? 'Goal & approvals' : 'Run' }}</span>
        </button>
      }
      <button class="tool-strip-item" role="menuitem" (click)="ui.paletteOpen.set(true); done.emit()">
        <pa-icon name="command" [size]="vertical() ? 16 : 18" />
        <span class="tool-strip-label">Commands</span>
        @if (vertical()) {
          <span class="kbd tool-strip-keys">{{ keys(shortcuts.palette) }}</span>
        }
      </button>
      <button class="tool-strip-item" role="menuitem" (click)="ui.settingsOpen.set(true); done.emit()">
        <pa-icon name="settings" [size]="vertical() ? 16 : 18" />
        <span class="tool-strip-label">Settings</span>
        @if (vertical()) {
          <span class="kbd tool-strip-keys">{{ keys(shortcuts.settings) }}</span>
        }
      </button>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolStrip {
  protected readonly work = inject(WorkbenchStore);
  protected readonly store = inject(AgentStore);
  protected readonly ui = inject(UiStore);
  /** A list, as it drops from the Tools button; a row otherwise. */
  readonly vertical = input(false);
  protected readonly keys = shortcut;
  protected readonly shortcuts = SHORTCUTS;
  /** Something was chosen: the strip closes. */
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

}
