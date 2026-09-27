import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LayoutService } from '../../core/layout';
import { NavigationService, WindowState } from '../../core/navigation';
import { SHORTCUTS, UiStore } from '../../core/ui';
import { Composer } from '../composer';
import { Conversation } from '../conversation';
import { Inspector } from '../inspector';
import { Icon } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';
import { DiagnosticExport } from '../parts/diagnostic-export';
import { PhaseStrip } from '../parts/phase-strip';
import { SessionSwitcher } from '../parts/session-switcher';
import { StatusBar } from '../parts/status-bar';
import { ToolDock } from '../parts/tool-dock';
import { MemoryProposals } from '../personal';

/** The rail's width; the workbench docks by what is left beside it. */
export const INSTRUMENT_RAIL = 48;

/**
 * Instrument: a tool, not a page. A rail of icons instead of a sidebar --
 * conversations behind one of them, the workbench's tools as the rest --
 * the run's phases on a strip across the top, and the engine, the
 * workspace and the rules the agent runs under in a status bar.
 */
@Component({
  selector: 'pa-shell-instrument',
  imports: [
    Composer,
    Conversation,
    DiagnosticExport,
    Icon,
    Inspector,
    MemoryProposals,
    PhaseStrip,
    SessionSwitcher,
    StatusBar,
    ToolDock,
    Tooltip,
  ],
  template: `
    <div
      class="shell-instrument"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [style.--right-col.px]="layout.right() === 'docked' ? layout.rightWidth() : 0"
      [style.--right-w.px]="layout.rightWidth()"
    >
      <nav class="inst-rail" aria-label="Navigation">
        <div class="inst-rail-head" data-tauri-drag-region="deep"></div>
        <pa-session-switcher [compact]="true" />
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
        <span class="inst-rail-sep" aria-hidden="true"></span>
        <pa-tool-dock [vertical]="true" />
        <span class="spacer"></span>
        <button class="icon-btn" (click)="ui.paletteOpen.set(true)" aria-label="Commands" paTooltip="Commands" [paTooltipKeys]="keys.palette">
          <pa-icon name="command" />
        </button>
        <button class="icon-btn" (click)="ui.settingsOpen.set(true)" aria-label="Settings" paTooltip="Settings" [paTooltipKeys]="keys.settings">
          <pa-icon name="settings" />
        </button>
      </nav>

      <main class="main inst-main" aria-label="Conversation">
        <header class="inst-head titlebar-row" data-tauri-drag-region="deep">
          <h1 class="inst-title truncate" data-tauri-drag-region="deep">{{ nav.title() }}</h1>
          <pa-phase-strip />
          <pa-diagnostic-export />
        </header>
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

      @if (layout.right() === 'docked') {
        <pa-inspector />
      }
      @if (layout.right() === 'overlay') {
        <div class="overlay-backdrop" (click)="layout.closeOverlays()" animate.leave="is-leaving"></div>
        <pa-inspector class="is-overlay" animate.leave="is-leaving" />
      }

      <pa-status-bar />
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InstrumentShell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly nav = inject(NavigationService);
  protected readonly ui = inject(UiStore);
  protected readonly win = inject(WindowState);
  protected readonly keys = SHORTCUTS;

  constructor() {
    this.layout.leftFixed.set(INSTRUMENT_RAIL);
  }
}
