import { ChangeDetectionStrategy, Component, ElementRef, inject, signal, viewChild } from '@angular/core';
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
import { RunControls } from '../parts/run-controls';
import { SessionSwitcher } from '../parts/session-switcher';
import { ToolStrip } from '../parts/tool-strip';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

/**
 * What a right click leaves to the system: text and whatever has its own
 * menu -- copy, paste, look up, a link, an image, the terminal, the graph.
 */
const NATIVE_MENU =
  'input, textarea, select, [contenteditable="true"], a, img, video, iframe, pre, code, .markdown, ' +
  '.selectable, .message-user, .terminal-surface, .graph3d-canvas, .diff, pa-popover';

/**
 * Focus: no chrome. The conversation fills the window; everything else
 * floats over it on glass, in one bar at the top right -- where you are, the
 * engine, and the tools -- leaving the window's corners clear. The tools open
 * as a row from that bar, or where a right click (a two-finger click) lands
 * on an empty part of the window. A selected tool gets its own column while
 * the conversation remains visible where space allows.
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
    RunControls,
    RunMetricsChip,
    SessionSwitcher,
    ToolStrip,
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
      (contextmenu)="context($event)"
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

      <div class="focus-bar glass">
        <pa-session-switcher />
        <span class="focus-bar-sep" aria-hidden="true"></span>
        <pa-diagnostic-export />
        <pa-context-meter />
        <pa-run-metrics />
        <pa-model-picker />
        <span class="focus-bar-sep" aria-hidden="true"></span>
        <button
          #toolsButton
          class="icon-btn focus-tools"
          [class.has-goal]="store.goalMode() && !store.chatMode()"
          [class.has-auto]="store.permissionMode() === 'auto' && !store.chatMode()"
          (click)="strip() === 'bar' ? strip.set(null) : strip.set('bar')"
          [attr.aria-expanded]="strip() === 'bar'"
          aria-haspopup="menu"
          aria-label="Tools"
          paTooltip="Tools · or right-click an empty spot"
        >
          <pa-icon name="grid" />
        </button>
      </div>

      <!-- Where a right click landed: the tools open from there. -->
      <span #pointerAnchor class="focus-pointer" [style.left.px]="pointer().x" [style.top.px]="pointer().y" aria-hidden="true"></span>

      @if (strip(); as from) {
        <pa-popover
          class="tool-strip-popover"
          [anchor]="from === 'bar' ? toolsButton : pointerAnchor"
          [anchorAlign]="from === 'bar' ? 'end' : 'start'"
          panelRole="menu"
          ariaLabel="Tools"
          [focusFirst]="true"
          (closed)="strip.set(null)"
          animate.leave="anim-pop-out"
        >
          <pa-tool-strip (done)="strip.set(null)" (run)="openRun(from)" />
        </pa-popover>
      }

      @if (ui.runControls(); as anchor) {
        <pa-run-controls [anchor]="anchor" (closed)="ui.runControls.set(null)" />
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
  /** The tools' row: open from the bar's button, or where a right click landed. */
  protected readonly strip = signal<'bar' | 'pointer' | null>(null);
  protected readonly pointer = signal({ x: 0, y: 0 });
  private readonly toolsRef = viewChild.required<ElementRef<HTMLElement>>('toolsButton');
  private readonly pointerRef = viewChild.required<ElementRef<HTMLElement>>('pointerAnchor');

  constructor() {
    // Focus uses standalone cards and starts with the conversation.
    this.work.focusMode.set(true);
    this.layout.leftFixed.set(0);
    this.layout.rightOpen.set(false);
    this.layout.rightPeek.set(false);
  }

  protected open(): boolean {
    return this.work.panelVisible() && this.work.visible().length > 0;
  }

  /**
   * A right click (a two-finger click) on an empty part of the window opens
   * the tools where it landed; on text, or anything with a menu of its own,
   * the system's menu is left alone.
   */
  protected context(event: MouseEvent): void {
    const target = event.target as HTMLElement | null;
    if (window.getSelection()?.toString()) return;
    if (!target || target.closest(NATIVE_MENU)) return;
    event.preventDefault();
    this.pointer.set({ x: event.clientX, y: event.clientY });
    this.strip.set(null);
    // A fresh strip, so it opens at the new spot with its own motion.
    queueMicrotask(() => this.strip.set('pointer'));
  }

  /** The run's controls, opened from where the strip was. */
  protected openRun(from: 'bar' | 'pointer'): void {
    this.strip.set(null);
    this.ui.runControls.set((from === 'bar' ? this.toolsRef() : this.pointerRef()).nativeElement);
  }
}
