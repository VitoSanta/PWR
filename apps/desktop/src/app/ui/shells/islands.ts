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

/**
 * Islands: every surface floats on a canvas, with room between. No top bar
 * -- the model, the context and the speed sit in the composer's dock, where
 * the next message is written -- and the workbench's cards are islands of
 * their own.
 */
@Component({
  selector: 'pa-shell-islands',
  imports: [
    Composer,
    ContextMeter,
    Conversation,
    DiagnosticExport,
    Icon,
    Inspector,
    MemoryProposals,
    ModelPicker,
    Rail,
    RunMetricsChip,
    Sidebar,
    Tooltip,
  ],
  template: `
    <div
      class="shell-islands"
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
        <pa-sidebar class="island" />
      } @else {
        <pa-rail class="island" />
      }

      <main class="main island islands-main" aria-label="Conversation">
        <header class="islands-head" data-tauri-drag-region="deep">
          <span class="dot" [class.dot-live]="store.turnActive()" aria-hidden="true"></span>
          <h1 class="truncate islands-title" data-tauri-drag-region="deep">{{ nav.title() }}</h1>
          <span class="spacer" data-tauri-drag-region="deep"></span>
          <pa-diagnostic-export />
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
        </header>
        @if (store.coreError()) {
          <div class="core-error banner banner-danger" role="alert">
            <pa-icon name="alert" [size]="16" />
            <span class="selectable">{{ store.coreError() }}</span>
          </div>
        }
        <pa-conversation />
        <pa-memory-proposals />
        <div class="islands-dock">
          <pa-composer />
          <div class="islands-chips">
            <pa-model-picker side="top" align="start" />
            <pa-context-meter side="top" align="start" />
            <pa-run-metrics side="top" align="start" />
          </div>
        </div>
      </main>

      @if (layout.right() === 'docked') {
        <pa-inspector class="islands-workbench" />
      }

      @if (layout.left() === 'overlay' || layout.right() === 'overlay') {
        <div class="overlay-backdrop" (click)="layout.closeOverlays()" animate.leave="is-leaving"></div>
      }
      @if (layout.left() === 'overlay') {
        <pa-sidebar class="is-overlay" animate.leave="is-leaving" />
      }
      @if (layout.right() === 'overlay') {
        <pa-inspector class="is-overlay islands-workbench" animate.leave="is-leaving" />
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class IslandsShell {
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
