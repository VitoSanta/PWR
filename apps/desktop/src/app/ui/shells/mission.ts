import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LayoutService } from '../../core/layout';
import { WindowState } from '../../core/navigation';
import { SHORTCUTS, UiStore } from '../../core/ui';
import { Composer } from '../composer';
import { ContextMeter } from '../context-meter';
import { Conversation } from '../conversation';
import { Inspector } from '../inspector';
import { Icon } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';
import { ModelPicker } from '../model-picker';
import { DiagnosticExport } from '../parts/diagnostic-export';
import { RunBoard } from '../parts/run-board';
import { SessionSwitcher } from '../parts/session-switcher';
import { ToolDock } from '../parts/tool-dock';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

/**
 * Mission: the run is the main surface. The conversation is a column on the
 * left -- what was asked and answered -- and the rest of the window is the
 * board of what the agent is doing, stage by stage. The workbench rises
 * from the bottom of the board, its tools on the dock beneath it.
 */
@Component({
  selector: 'pa-shell-mission',
  imports: [
    Composer,
    ContextMeter,
    Conversation,
    DiagnosticExport,
    Icon,
    Inspector,
    MemoryProposals,
    ModelPicker,
    RunBoard,
    RunMetricsChip,
    SessionSwitcher,
    ToolDock,
    Tooltip,
  ],
  template: `
    <div
      class="shell-mission"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.has-workbench]="open()"
    >
      <header class="mission-head titlebar-row" data-tauri-drag-region="deep">
        <pa-session-switcher />
        <span class="spacer" data-tauri-drag-region="deep"></span>
        <pa-diagnostic-export />
        <pa-context-meter />
        <pa-run-metrics />
        <pa-model-picker />
        <span class="topbar-sep" aria-hidden="true"></span>
        <button class="icon-btn" (click)="ui.paletteOpen.set(true)" aria-label="Commands" paTooltip="Commands" [paTooltipKeys]="keys.palette">
          <pa-icon name="command" />
        </button>
        <button class="icon-btn" (click)="ui.settingsOpen.set(true)" aria-label="Settings" paTooltip="Settings" [paTooltipKeys]="keys.settings">
          <pa-icon name="settings" />
        </button>
      </header>

      <main class="main mission-chat" aria-label="Conversation">
        @if (store.coreError()) {
          <div class="core-error banner banner-danger" role="alert">
            <pa-icon name="alert" [size]="16" />
            <span class="selectable">{{ store.coreError() }}</span>
          </div>
        }
        <pa-conversation />
        <pa-memory-proposals />
        <pa-composer />
      </main>

      <section class="mission-stage" aria-label="The run and the tools">
        <pa-run-board />
        @if (open()) {
          <pa-inspector class="mission-workbench" animate.leave="is-leaving" />
        }
        <div class="mission-dock">
          <pa-tool-dock [small]="true" />
          <span class="spacer"></span>
          <button
            class="btn btn-sm btn-ghost"
            (click)="layout.toggleRight()"
            [attr.aria-pressed]="open()"
            [paTooltipKeys]="keys.toggleInspector"
            paTooltip="Show or hide the workbench"
          >
            <pa-icon [name]="open() ? 'chevron-down' : 'chevron-up'" [size]="14" />
            {{ open() ? 'Hide tools' : 'Tools' }}
          </button>
        </div>
      </section>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MissionShell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly ui = inject(UiStore);
  protected readonly win = inject(WindowState);
  protected readonly keys = SHORTCUTS;

  constructor() {
    // The workbench opens under the board, never beside the conversation.
    this.layout.leftFixed.set(0);
  }

  protected open(): boolean {
    return this.layout.right() !== 'hidden';
  }
}
