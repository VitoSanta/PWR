import { MatButtonModule } from '@angular/material/button';
import { ChangeDetectionStrategy, Component, ElementRef, inject, signal, viewChild } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LayoutService } from '../../core/layout';
import { WorkbenchStore } from '../../core/workbench';
import { WindowState } from '../../core/navigation';
import { SHORTCUTS, UiStore, roveFocus } from '../../core/ui';
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
import { ToolWheel } from '../parts/tool-wheel';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

/**
 * What a right click leaves alone: text and whatever has its own menu --
 * copy, paste, look up, a link, an image -- the bar, and what a tool card
 * shows; the wheel opens over empty space, in the conversation or around
 * the tools, and on the cards' headers.
 */
const NATIVE_MENU =
  'input, textarea, select, [contenteditable="true"], a, img, video, iframe, pre, code, .markdown, ' +
  '.selectable, .message-user, .terminal-surface, .graph3d-canvas, .diff, pa-popover, .wb-card-body, .composer';

/**
 * Focus: no chrome. The conversation fills the window; everything else is
 * in the message bar -- the tools and where you are at its left, the run's
 * state and the model at its right -- as Google's assistants keep theirs.
 * The tools open as a list from that bar, or as a wheel where a right
 * click (a two-finger click) lands on an empty part of the window. A selected tool gets its own column while
 * the conversation remains visible where space allows.
 */
@Component({
  selector: 'pa-shell-focus',
  host: { '(document:keydown.escape)': 'leaveFullScreen($event)' },
  imports: [MatButtonModule, 
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
    ToolWheel,
    Tooltip,
  ],
  template: `
    <div
      class="shell-focus"
      [class.has-tool]="open()"
      [class.tool-page]="open() && layout.right() !== 'docked'"
      [class.tool-full]="open() && !!work.focused()"
      [class.is-mac]="win.isMac"
      [class.is-native]="win.native"
      [class.is-fullscreen]="win.fullscreen()"
      [class.is-resizing]="layout.resizing()"
      [style.--right-w.px]="layout.rightWidth()"
      (contextmenu)="context($event)"
    >
      <div class="focus-drag" data-tauri-drag-region="deep"></div>

      <!-- Navigation where a Mac app keeps it: the conversations, beside the
           window's buttons; and, in development builds, the trace export. -->
      <nav class="focus-nav" aria-label="Conversations">
        <pa-session-switcher [compact]="true" />
        <pa-diagnostic-export />
      </nav>

      <main #main class="main focus-main" aria-label="Conversation">
        @if (store.coreError()) {
          <div class="core-error banner banner-danger" role="alert">
            <pa-icon name="alert" [size]="16" />
            <span class="selectable">{{ store.coreError() }}</span>
          </div>
        }
        <pa-conversation />
        <pa-memory-proposals />
        <pa-composer>
          <!-- Everything lives in the message bar: the tools and the
               conversation at its left, the run's state and the model at
               its right. -->
          <!-- The message bar holds actions only: attach (the composer's
               own) and the tools at the left; at the right the engine --
               the context's ring and the model, whose panel holds thinking
               and speed -- then stop and send. -->
          <div paComposerStart class="composer-tools">
            <button matIconButton
              #toolsButton
              type="button"
              class="icon-btn composer-tools-button"
              [class.has-goal]="store.goalMode() && !store.chatMode()"
              [class.has-auto]="store.permissionMode() === 'auto' && !store.chatMode()"
              (click)="strip() === 'bar' ? strip.set(null) : openStrip()"
              [attr.aria-expanded]="strip() === 'bar'"
              aria-haspopup="menu"
              aria-label="Options"
              paTooltip="Options · right-click an empty spot for the tools"
            >
              <pa-icon name="settings" />
            </button>
          </div>
          <div paComposerEnd class="composer-engine">
            <pa-context-meter />
            <pa-model-picker [compact]="true" />
          </div>
        </pa-composer>
      </main>


      @if (strip() === 'bar') {
        @for (opening of [stripOpenings()]; track opening) {
          <pa-popover
            class="tool-strip-popover"
            [anchor]="toolsButton"
            anchorAlign="start"
            panelRole="menu"
            ariaLabel="Options"
            (keydown)="stripKeys($event)"
            (closed)="strip.set(null)"
            animate.leave="anim-pop-out"
          >
            <pa-tool-strip (done)="strip.set(null)" (run)="openRun()" />
          </pa-popover>
        }
      }

      <!-- The run's figures, opened from the model's panel. -->
      <pa-run-metrics [panelOnly]="true" side="top" />

      @if (ui.runControls(); as anchor) {
        <pa-run-controls [anchor]="anchor" (closed)="ui.runControls.set(null)" />
      }

      @if (open()) {
        <pa-inspector class="focus-workbench" />
      }

      <!-- A right click: the tools as a wheel around where it landed. After the
           tools, so that in a view transition it is painted over them. -->
      @if (strip() === 'pointer') {
        <pa-tool-wheel [at]="pointer()" (close)="strip.set(null)" animate.leave="is-leaving" />
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
  protected readonly stripOpenings = signal(0);
  private readonly toolsRef = viewChild.required<ElementRef<HTMLElement>, ElementRef<HTMLElement>>('toolsButton', { read: ElementRef });

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

  /** Escape leaves a tool's full screen, when nothing else took the key. */
  protected leaveFullScreen(event: Event): void {
    const full = this.work.focused();
    if (!full || event.defaultPrevented || !this.open()) return;
    event.preventDefault();
    this.work.maximize(full);
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
    this.stripOpenings.update((count) => count + 1);
    this.strip.set('pointer');
  }

  /**
   * The tools open with focus on themselves, not on the first (whose
   * tooltip would pop up under the pointer); the arrow keys go in -- up and
   * down the list, along the row.
   */
  protected stripKeys(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, '[role^=menuitem]', 'vertical');
  }

  protected openStrip(): void {
    this.stripOpenings.update((count) => count + 1);
    this.strip.set('bar');
  }

  /** The run's controls, opened from where the strip was. */
  protected openRun(): void {
    this.strip.set(null);
    this.ui.runControls.set(this.toolsRef().nativeElement);
  }
}
