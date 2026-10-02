import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { LayoutService } from '../core/layout';
import { WindowState } from '../core/navigation';
import { SHORTCUTS, UiStore } from '../core/ui';
import { WorkbenchStore } from '../core/workbench';
import { Composer } from './composer';
import { Conversation } from './conversation';
import { Inspector } from './inspector';
import { Icon } from './kit/icon';
import { Tooltip } from './kit/tooltip';
import { DiagnosticExport } from './parts/diagnostic-export';
import { RunControls } from './parts/run-controls';
import { MemoryProposals } from './personal';
import { RunMetricsChip } from './run-metrics';
import { Sidebar } from './sidebar';
import { TraceVisibilityControl } from './trace';

/**
 * The window: the sidebar, the conversation in a column of its own, and
 * the inspector's tabs on the right. Each side docks when the window has
 * room and otherwise opens over the conversation (LayoutService).
 */
@Component({
  selector: 'pa-shell',
  imports: [
    Composer,
    Conversation,
    DiagnosticExport,
    Icon,
    Inspector,
    MemoryProposals,
    RunControls,
    RunMetricsChip,
    Sidebar,
    TraceVisibilityControl,
    Tooltip,
  ],
  template: `
    <div
      class="shell"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [class.left-docked]="layout.left() === 'docked'"
      [style.--left-col]="layout.left() === 'docked' ? layout.leftWidth() + 'px' : '0px'"
      [style.--right-col]="layout.right() === 'docked' ? layout.rightWidth() + 'px' : '0px'"
      [style.--left-w.px]="layout.leftWidth()"
      [style.--right-w.px]="layout.rightWidth()"
    >
      @if (layout.left() !== 'hidden') {
        <pa-sidebar [class.is-overlay]="layout.left() === 'overlay'" animate.leave="is-leaving" />
      }

      <main class="main" aria-label="Conversation">
        <header class="main-head titlebar-row" data-tauri-drag-region="deep">
          @if (layout.left() !== 'docked') {
            <button
              class="icon-btn icon-btn-sm"
              (click)="layout.toggleLeft()"
              [attr.aria-expanded]="layout.left() === 'overlay'"
              aria-label="Show sidebar"
              paTooltip="Show sidebar"
              [paTooltipKeys]="keys.toggleSidebar"
            >
              <pa-icon name="panel-left" [size]="16" />
            </button>
          }
          <span class="spacer" data-tauri-drag-region="deep"></span>
          <span class="main-tools">
            <pa-diagnostic-export />
            <pa-trace-visibility />
            <pa-run-metrics />
            @if (!work.panelVisible()) {
              <button
                class="icon-btn icon-btn-sm"
                (click)="layout.toggleRight()"
                aria-label="Show the inspector"
                [paTooltip]="'Changes, terminal, preview, files and checks'"
                [paTooltipKeys]="keys.toggleInspector"
              >
                <pa-icon name="panel-right" [size]="16" />
                @if (changes()) {
                  <span class="main-tools-count num">{{ changes() }}</span>
                }
              </button>
            }
          </span>
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

      @if (work.panelVisible()) {
        <pa-inspector [class.is-overlay]="layout.right() === 'overlay'" animate.leave="is-leaving" />
      }

      @if (layout.left() === 'overlay' || layout.right() === 'overlay') {
        <div class="overlay-backdrop" (click)="layout.closeOverlays()" animate.leave="is-leaving"></div>
      }

      @if (ui.runControls(); as anchor) {
        <pa-run-controls [anchor]="anchor" (closed)="ui.runControls.set(null)" />
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Shell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly work = inject(WorkbenchStore);
  protected readonly ui = inject(UiStore);
  protected readonly win = inject(WindowState);
  protected readonly keys = SHORTCUTS;
  protected readonly changes = computed(() => (this.store.chatMode() ? 0 : this.store.changes().length));
}
