import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { PermissionRequest } from '../core/model';
import { Dialog } from './kit/dialog';
import { Icon } from './kit/icon';

/**
 * The core asks before an action outside the current permissions. It must
 * be answered, so the dialog has no close button and Escape does nothing.
 */
@Component({
  selector: 'pa-permission',
  imports: [Dialog, Icon],
  template: `
    @if (store.permission(); as asked) {
      <pa-dialog
        dialogRole="alertdialog"
        labelledBy="permission-title"
        describedBy="permission-subject"
        [dismissible]="false"
        size="md"
        animate.leave="is-leaving"
      >
        <div class="dialog-header">
          <span class="dialog-icon tone-warning"><pa-icon name="shield" [size]="18" /></span>
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="permission-title">PWR asks for permission</h2>
            <p class="dialog-description">The next action needs your approval before it runs.</p>
          </div>
        </div>
        <div class="dialog-body">
          <p class="dialog-subject" id="permission-subject" tabindex="-1" data-autofocus>{{ asked.title }}</p>
          @if (asked.approval) {
            <p class="fine">{{ approvalLabel(asked.approval) }}</p>
          }
        </div>
        <div class="dialog-footer">
          @for (option of ordered(asked.options); track option.optionId) {
            <button
              [class]="option.kind.startsWith('reject') ? 'btn btn-danger-quiet' : option.kind === 'allow_once' ? 'btn btn-primary' : 'btn'"
              (click)="store.answerPermission(option.optionId)"
            >
              {{ option.name }}
            </button>
          }
        </div>
      </pa-dialog>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Permission {
  protected readonly store = inject(AgentStore);

  /** The core's approval kind (`pwr_tools::Approval`, snake case), in words. */
  protected approvalLabel(kind: string): string {
    const labels: Record<string, string> = {
      dependency_change: 'Changes a dependency manifest or lockfile',
      verifier_proposal: "Adopts a command as this workspace's check",
      history_rewrite: 'Rewrites version-control history',
      publish: 'Publishes a package or pushes to a remote',
      network_access: 'Reaches the network',
      local_service: 'Starts a service on this machine',
      toolchain_install: 'Runs a program this workspace does not list',
      container_engine: 'Uses Docker — containers run outside the sandbox',
    };
    return labels[kind] ?? kind.replaceAll('_', ' ');
  }

  /**
   * Every dialog footer reads the same way: the way out on the left, the
   * primary action last, on the right. The core lists the options primary
   * first, so they are put in footer order here.
   */
  protected ordered(options: PermissionRequest['options']): PermissionRequest['options'] {
    const rank = (kind: string) => (kind.startsWith('reject') ? 0 : kind === 'allow_once' ? 2 : 1);
    return [...options].sort((a, b) => rank(a.kind) - rank(b.kind));
  }
}
