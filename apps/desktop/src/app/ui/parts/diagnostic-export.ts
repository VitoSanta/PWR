import { ChangeDetectionStrategy, Component, inject, isDevMode } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { bridge, inTauri } from '../../core/bridge';
import { ToastService } from '../../core/ui';
import { Icon } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';

/** Development builds only: the chat and the model's raw trace, to a file. */
@Component({
  selector: 'pa-diagnostic-export',
  imports: [Icon, Tooltip],
  template: `
    @if (available) {
      <button
        class="icon-btn"
        (click)="export()"
        [disabled]="!store.timeline().length || store.turnActive()"
        aria-label="Export development chat diagnostic"
        paTooltip="Export chat and model raw trace (development only)"
      >
        <pa-icon name="download" />
      </button>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DiagnosticExport {
  protected readonly store = inject(AgentStore);
  private readonly toast = inject(ToastService);
  protected readonly available = isDevMode() && inTauri();

  protected async export(): Promise<void> {
    const timeline = this.store.timeline();
    if (!this.available || !timeline.length || this.store.turnActive()) return;
    const destination = await bridge.pickDebugExport();
    if (!destination) return;
    const from = timeline[0].at;
    const to = timeline[timeline.length - 1].at + 2000;
    try {
      await bridge.debugExportChat(destination, {
        session_id: this.store.sessionId(),
        workspace: this.store.workspace(),
        model: this.store.model(),
        timeline,
        core_log: this.store.logLines().filter((line) => line.at >= from && line.at <= to),
      });
      this.toast.show('Diagnostic chat exported.', 'success');
    } catch (error) {
      this.toast.show(`Export failed: ${String(error)}`, 'danger', 6000);
    }
  }
}
