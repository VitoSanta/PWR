import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LayoutService } from '../../core/layout';
import { WorkbenchStore } from '../../core/workbench';
import { WindowState } from '../../core/navigation';
import { SHORTCUTS, UiStore } from '../../core/ui';
import { Composer } from '../composer';
import { ContextMeter } from '../context-meter';
import { Conversation } from '../conversation';
import { Inspector } from '../inspector';
import { Icon } from '../kit/icon';
import { Popover } from '../kit/popover';
import { Tooltip } from '../kit/tooltip';
import { ModelPicker } from '../model-picker';
import { DiagnosticExport } from '../parts/diagnostic-export';
import { SessionSwitcher } from '../parts/session-switcher';
import { ToolDock } from '../parts/tool-dock';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

/**
 * What the floating dock takes at the left edge: its inset, its width and a
 * gap. The conversation is centred in what is left of the window, and the
 * tools dock by that room.
 */
const RAIL = 72;

/**
 * Focus: no chrome. The conversation fills the window; everything else
 * floats over it on glass -- where you are at the top, the engine at the
 * top right, and the tools on a dock at the left edge. A selected tool gets
 * its own column while the conversation remains visible where space allows.
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
    Popover,
    RunMetricsChip,
    SessionSwitcher,
    ToolDock,
    Tooltip,
  ],
  template: `
    <div
      class="shell-focus"
      [class.has-tool]="open()"
      [class.tool-page]="open() && layout.right() !== 'docked'"
      [class.is-mac]="win.isMac"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [style.--right-w.px]="layout.rightWidth()"
      [style.--focus-rail.px]="rail"
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
        @if (!store.chatMode()) {
          <button
            #runControlsButton
            class="icon-btn focus-run-trigger"
            [class.has-goal]="store.goalMode()"
            [class.has-auto]="store.permissionMode() === 'auto'"
            [attr.aria-expanded]="runControlsOpen()"
            [attr.aria-label]="
              'Run controls: Goal ' +
              (store.goalMode() ? 'on' : 'off') +
              ', Auto-approve ' +
              (store.permissionMode() === 'auto' ? 'on' : 'off')
            "
            aria-haspopup="dialog"
            (click)="runControlsOpen.update((open) => !open)"
            paTooltip="Goal mode and approvals"
          >
            <pa-icon name="shield-check" />
          </button>
        }
        <button
          class="icon-btn"
          (click)="ui.paletteOpen.set(true)"
          aria-label="Commands"
          paTooltip="Commands"
          [paTooltipKeys]="keys.palette"
        >
          <pa-icon name="command" />
        </button>
        <button
          class="icon-btn"
          (click)="ui.settingsOpen.set(true)"
          aria-label="Settings"
          paTooltip="Settings"
          [paTooltipKeys]="keys.settings"
        >
          <pa-icon name="settings" />
        </button>
      </nav>

      @if (runControlsOpen() && runControlsButton(); as runAnchor) {
        <pa-popover
          class="focus-run-popover"
          [anchor]="runAnchor.nativeElement"
          side="right"
          anchorAlign="start"
          width="300px"
          ariaLabel="Run controls"
          (closed)="runControlsOpen.set(false)"
          animate.leave="anim-pop-out"
        >
          <div class="popover-head"><h2 class="popover-title">Run controls</h2></div>
          <div class="focus-run-options">
            <button
              class="focus-run-option"
              [attr.aria-pressed]="store.goalMode()"
              (click)="store.goalMode.set(!store.goalMode())"
            >
              <span class="focus-run-copy">
                <strong>Goal mode</strong>
                <small>Keep working until the goal is verified.</small>
              </span>
              <span class="switch" aria-hidden="true"></span>
            </button>
            <button
              class="focus-run-option tone-warning"
              [attr.aria-pressed]="store.permissionMode() === 'auto'"
              (click)="store.setPermissionMode(store.permissionMode() === 'auto' ? 'ask' : 'auto')"
            >
              <span class="focus-run-copy">
                <strong>Auto-approve</strong>
                <small>{{
                  store.permissionMode() === 'auto'
                    ? 'Permissions are granted automatically.'
                    : 'PWR asks before sensitive actions.'
                }}</small>
              </span>
              <span class="switch" aria-hidden="true"></span>
            </button>
          </div>
        </pa-popover>
      }

      @if (open()) {
        <pa-inspector class="focus-workbench" animate.leave="is-leaving" />
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class FocusShell {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly work = inject(WorkbenchStore);
  protected readonly ui = inject(UiStore);
  protected readonly win = inject(WindowState);
  protected readonly keys = SHORTCUTS;
  protected readonly rail = RAIL;
  protected readonly runControlsOpen = signal(false);
  protected readonly runControlsButton = viewChild<ElementRef<HTMLElement>>('runControlsButton');

  constructor() {
    // Focus uses standalone cards and starts with the conversation.
    this.work.focusMode.set(true);
    this.layout.leftFixed.set(RAIL);
    this.layout.rightOpen.set(false);
    this.layout.rightPeek.set(false);
  }

  protected open(): boolean {
    return this.work.panelVisible() && this.work.visible().length > 0;
  }
}
