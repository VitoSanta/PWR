import { ChangeDetectionStrategy, Component, inject, output } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { SHORTCUTS, UiStore, shortcut } from '../../core/ui';
import { TRACE_VISIBILITIES, TraceVisibility } from '../../core/trace';
import { Icon, IconName } from '../kit/icon';

/**
 * The message bar's options: how much of the run the conversation shows,
 * the run's controls, the commands and the settings. The tools themselves
 * are not here -- they are on the wheel a right click (a two-finger click)
 * opens.
 */
@Component({
  selector: 'pa-tool-strip',
  imports: [Icon],
  template: `
    <div class="tool-strip is-vertical" role="none">
      <!-- How much of the run the conversation shows: a view setting, here
           and in Settings. -->
      <span class="tool-strip-heading" role="presentation">Trace</span>
      @for (option of traceOptions; track option.value) {
        <button
          class="tool-strip-item"
          role="menuitemradio"
          [attr.aria-checked]="store.traceVisibility() === option.value"
          (click)="store.setTraceVisibility(option.value); done.emit()"
        >
          <pa-icon [name]="store.traceVisibility() === option.value ? 'check' : traceIcon(option.value)" [size]="16" />
          <span class="tool-strip-label">{{ option.label }}</span>
        </button>
      }
      <span class="tool-strip-sep" aria-hidden="true"></span>
      @if (!store.chatMode()) {
        <button
          class="tool-strip-item"
          role="menuitem"
          [class.has-goal]="store.goalMode()"
          [class.has-auto]="store.permissionMode() === 'auto'"
          (click)="run.emit()"
        >
          <pa-icon name="zap" [size]="16" />
          <span class="tool-strip-label">Goal &amp; approvals</span>
        </button>
      }
      <button class="tool-strip-item" role="menuitem" (click)="ui.paletteOpen.set(true); done.emit()">
        <pa-icon name="command" [size]="16" />
        <span class="tool-strip-label">Commands</span>
        <span class="kbd tool-strip-keys">{{ keys(shortcuts.palette) }}</span>
      </button>
      <button class="tool-strip-item" role="menuitem" (click)="ui.settingsOpen.set(true); done.emit()">
        <pa-icon name="settings" [size]="16" />
        <span class="tool-strip-label">All settings</span>
        <span class="kbd tool-strip-keys">{{ keys(shortcuts.settings) }}</span>
      </button>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolStrip {
  protected readonly store = inject(AgentStore);
  protected readonly ui = inject(UiStore);
  protected readonly keys = shortcut;
  protected readonly traceOptions = TRACE_VISIBILITIES;

  protected traceIcon(value: TraceVisibility): IconName {
    return value === 'compact' ? 'eye' : value === 'detailed' ? 'list-plus' : 'scroll-text';
  }
  protected readonly shortcuts = SHORTCUTS;
  /** Something was chosen: the strip closes. */
  readonly done = output<void>();
  /** The run's controls were asked for. */
  readonly run = output<void>();


}
