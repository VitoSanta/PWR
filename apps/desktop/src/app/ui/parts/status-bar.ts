import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { NavigationService } from '../../core/navigation';
import { SHORTCUTS, UiStore, shortcut } from '../../core/ui';
import { ContextMeter } from '../context-meter';
import { Icon } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';
import { ModelPicker } from '../model-picker';
import { RunMetricsChip } from '../run-metrics';

/**
 * An editor's status bar: where the agent works and under which rules on
 * the left, the engine on the right. The chips open upward.
 */
@Component({
  selector: 'pa-status-bar',
  imports: [ContextMeter, Icon, ModelPicker, RunMetricsChip, Tooltip],
  template: `
    <footer class="status-bar">
      <span class="status-item" [paTooltip]="nav.coreDetail()">
        <span class="dot" [class]="'dot ' + nav.coreDot()" aria-hidden="true"></span>
        {{ nav.coreLabel() }}
      </span>
      @if (store.chatMode()) {
        <span class="status-item"><pa-icon name="lock" [size]="13" /> Chat · read-only</span>
      } @else {
        <button class="status-item" (click)="store.chooseWorkspace()" [disabled]="nav.switching()" [paTooltip]="store.workspace() || 'Choose a folder'">
          <pa-icon name="folder" [size]="13" /> {{ nav.folderName() }}
        </button>
        <span
          class="status-item"
          [class.tone-warning]="store.permissionMode() === 'auto'"
          [paTooltip]="store.permissionMode() === 'auto' ? 'Actions run without asking' : 'Actions outside the rules ask first'"
        >
          <pa-icon name="shield" [size]="13" /> {{ store.permissionMode() === 'auto' ? 'Auto-approve' : 'Ask' }}
        </span>
        <span class="status-item" [class.tone-danger]="!store.sandboxed()">
          <pa-icon [name]="store.sandboxed() ? 'lock' : 'alert'" [size]="13" /> {{ store.sandboxed() ? 'Sandboxed' : 'Not sandboxed' }}
        </span>
      }
      <span class="spacer"></span>
      <pa-context-meter side="top" />
      <pa-run-metrics side="top" />
      <pa-model-picker side="top" />
      <button class="status-item" (click)="ui.paletteOpen.set(true)" aria-label="Commands">
        <span class="kbd">{{ shortcut(keys.palette) }}</span>
      </button>
    </footer>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StatusBar {
  protected readonly store = inject(AgentStore);
  protected readonly nav = inject(NavigationService);
  protected readonly ui = inject(UiStore);
  protected readonly keys = SHORTCUTS;
  protected readonly shortcut = shortcut;
}
