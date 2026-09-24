import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
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
            <p class="fine">Approval: {{ asked.approval }}</p>
          }
        </div>
        <div class="dialog-footer">
          @for (option of asked.options; track option.optionId) {
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
}
