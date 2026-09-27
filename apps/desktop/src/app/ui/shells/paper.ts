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
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

/**
 * Paper: the conversation as a document. No sidebar -- the conversation's
 * title is the switcher -- the run's phases as notes in the margin, and the
 * workbench as a drawer that slides over the page and back.
 */
@Component({
  selector: 'pa-shell-paper',
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
    Tooltip,
  ],
  template: `
    <div
      class="shell-paper"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [style.--right-w.px]="layout.rightWidth()"
    >
      <header class="paper-head titlebar-row" data-tauri-drag-region="deep">
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
        <button
          class="icon-btn"
          (click)="layout.toggleRight()"
          [attr.aria-pressed]="open()"
          aria-label="Workbench"
          paTooltip="Workbench"
          [paTooltipKeys]="keys.toggleInspector"
        >
          <pa-icon name="panel-right" />
        </button>
      </header>

      <div class="paper-body">
        <aside class="paper-margin" aria-label="The run">
          <pa-phase-strip [vertical]="true" />
        </aside>
        <main class="main paper-main" aria-label="Conversation">
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
      </div>

      @if (open()) {
        <div class="overlay-backdrop" (click)="close()" animate.leave="is-leaving"></div>
        <pa-inspector class="is-overlay paper-drawer" animate.leave="is-leaving" />
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PaperShell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly ui = inject(UiStore);
  protected readonly win = inject(WindowState);
  protected readonly keys = SHORTCUTS;

  constructor() {
    // Nothing is docked beside the page: the drawer floats over it, and
    // starts closed, so the page is what opens.
    this.layout.leftFixed.set(0);
    this.layout.rightOpen.set(false);
    this.layout.rightPeek.set(false);
  }

  protected open(): boolean {
    return this.layout.right() !== 'hidden';
  }

  protected close(): void {
    if (this.layout.right() === 'overlay') this.layout.closeOverlays();
    else this.layout.toggleRight();
  }
}
