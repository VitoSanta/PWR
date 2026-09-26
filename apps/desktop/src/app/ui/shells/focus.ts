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
import { PhaseStrip } from '../parts/phase-strip';
import { SessionSwitcher } from '../parts/session-switcher';
import { ToolDock } from '../parts/tool-dock';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

/**
 * Focus: no chrome. The conversation fills the window; everything else
 * floats over it on glass -- where you are and the run's phases at the top,
 * the engine at the top right, the tools on a dock at the left edge -- and
 * the workbench is a panel that hovers over the page until it is closed.
 */
@Component({
  selector: 'pa-shell-focus',
  imports: [
    Composer,
    ContextMeter,
    Conversation,
    DiagnosticExport,
    Icon,
    Inspector,
    MemoryProposals,
    ModelPicker,
    PhaseStrip,
    RunMetricsChip,
    SessionSwitcher,
    ToolDock,
    Tooltip,
  ],
  template: `
    <div
      class="shell-focus"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [style.--right-w.px]="layout.rightWidth()"
    >
      <div class="focus-drag" data-tauri-drag-region="deep"></div>

      <main class="main focus-main" aria-label="Conversation">
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

      <div class="focus-hud glass">
        <pa-session-switcher />
        <span class="focus-hud-sep" aria-hidden="true"></span>
        <pa-phase-strip />
      </div>

      <div class="focus-engine glass">
        <pa-diagnostic-export />
        <pa-context-meter />
        <pa-run-metrics />
        <pa-model-picker />
      </div>

      <nav class="focus-dock glass" aria-label="Tools">
        <button
          class="icon-btn"
          (click)="store.newConversation()"
          [disabled]="store.turnActive()"
          aria-label="New conversation"
          paTooltip="New conversation"
          [paTooltipKeys]="keys.newConversation"
        >
          <pa-icon name="square-pen" />
        </button>
        <span class="focus-dock-sep" aria-hidden="true"></span>
        <pa-tool-dock [vertical]="true" />
        <span class="focus-dock-sep" aria-hidden="true"></span>
        <button class="icon-btn" (click)="ui.paletteOpen.set(true)" aria-label="Commands" paTooltip="Commands" [paTooltipKeys]="keys.palette">
          <pa-icon name="command" />
        </button>
        <button class="icon-btn" (click)="ui.settingsOpen.set(true)" aria-label="Settings" paTooltip="Settings" [paTooltipKeys]="keys.settings">
          <pa-icon name="settings" />
        </button>
      </nav>

      @if (open()) {
        <pa-inspector class="is-overlay focus-workbench glass" animate.leave="is-leaving" />
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class FocusShell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly ui = inject(UiStore);
  protected readonly win = inject(WindowState);
  protected readonly keys = SHORTCUTS;

  constructor() {
    // Nothing docks: the workbench hovers, and starts closed.
    this.layout.leftFixed.set(0);
    this.layout.rightOpen.set(false);
    this.layout.rightPeek.set(false);
  }

  protected open(): boolean {
    return this.layout.right() !== 'hidden';
  }
}
