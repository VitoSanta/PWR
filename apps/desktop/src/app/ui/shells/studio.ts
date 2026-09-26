import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LayoutService, RAIL } from '../../core/layout';
import { NavigationService, WindowState } from '../../core/navigation';
import { SHORTCUTS } from '../../core/ui';
import { Composer } from '../composer';
import { ContextMeter } from '../context-meter';
import { Conversation } from '../conversation';
import { Inspector } from '../inspector';
import { Icon } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';
import { ModelPicker } from '../model-picker';
import { DiagnosticExport } from '../parts/diagnostic-export';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';
import { Rail, Sidebar } from '../sidebar';

/** Studio: the sidebar, the conversation and the workbench, side by side. */
@Component({
  selector: 'pa-shell-studio',
  imports: [
    Sidebar,
    Rail,
    Conversation,
    Composer,
    MemoryProposals,
    Inspector,
    ContextMeter,
    RunMetricsChip,
    ModelPicker,
    DiagnosticExport,
    Icon,
    Tooltip,
  ],
  template: `
    <div
      class="shell"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [class.left-docked]="layout.left() === 'docked'"
      [style.--left-col.px]="layout.left() === 'docked' ? layout.leftWidth() : rail"
      [style.--right-col.px]="layout.right() === 'docked' ? layout.rightWidth() : 0"
      [style.--left-w.px]="layout.leftWidth()"
      [style.--right-w.px]="layout.rightWidth()"
    >
      @if (layout.left() === 'docked') {
        <pa-sidebar />
      } @else {
        <pa-rail />
      }

      <main class="main" aria-label="Conversation">
        <header class="topbar titlebar-row" data-tauri-drag-region="deep">
          <div class="topbar-title" data-tauri-drag-region="deep">
            <span class="dot" [class.dot-live]="store.turnActive()" aria-hidden="true"></span>
            <h1 class="truncate topbar-heading">{{ nav.title() }}</h1>
            @if (store.turnActive()) {
              <span class="badge badge-info">Working</span>
            } @else if (store.actionCount()) {
              <span class="t-meta num">{{ store.actionCount() }} action{{ store.actionCount() === 1 ? '' : 's' }}</span>
            }
          </div>
          <div class="topbar-actions">
            <pa-diagnostic-export />
            <pa-context-meter />
            <pa-run-metrics />
            <pa-model-picker />
            <span class="topbar-sep" aria-hidden="true"></span>
            <button
              class="icon-btn"
              (click)="layout.toggleRight()"
              [attr.aria-pressed]="layout.right() !== 'hidden'"
              aria-label="Workbench"
              paTooltip="Workbench"
              [paTooltipKeys]="keys.toggleInspector"
            >
              <pa-icon name="panel-right" />
            </button>
          </div>
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

      @if (layout.left() === 'overlay' || layout.right() === 'overlay') {
        <div class="overlay-backdrop" (click)="layout.closeOverlays()" animate.leave="is-leaving"></div>
      }
      @if (layout.left() === 'overlay') {
        <pa-sidebar class="is-overlay" animate.leave="is-leaving" />
      }
      @if (layout.right() === 'overlay') {
        <pa-inspector class="is-overlay" animate.leave="is-leaving" />
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StudioShell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly nav = inject(NavigationService);
  protected readonly win = inject(WindowState);
  protected readonly rail = RAIL;
  protected readonly keys = SHORTCUTS;

  constructor() {
    this.layout.leftFixed.set(null);
  }
}
