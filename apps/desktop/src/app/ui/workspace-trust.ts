import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { Dialog } from './kit/dialog';
import { Icon } from './kit/icon';

@Component({
  selector: 'pa-workspace-trust',
  imports: [Dialog, Icon],
  template: `
    @if (store.pendingWorkspaceTrust(); as folder) {
      <pa-dialog
        dialogRole="alertdialog"
        labelledBy="trust-title"
        describedBy="trust-description"
        [dismissible]="!store.trustingWorkspace()"
        (closed)="store.cancelWorkspaceTrust()"
        size="md"
        animate.leave="is-leaving"
      >
        <div class="dialog-header">
          <span class="dialog-icon tone-warning"><pa-icon name="shield" [size]="18" /></span>
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="trust-title">Trust this folder?</h2>
            <p class="dialog-description" id="trust-description">
              PWR can read and edit files and run commands here. Review this folder and its project
              instructions before continuing. Your Ask or Auto-approve setting stays as configured.
            </p>
          </div>
        </div>
        <div class="dialog-body">
          <p class="dialog-subject">{{ folder }}</p>
          @if (store.workspaceTrustError()) {
            <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ store.workspaceTrustError() }}</p>
          }
        </div>
        <div class="dialog-footer">
          <button class="btn" (click)="store.cancelWorkspaceTrust()" [disabled]="store.trustingWorkspace()" data-autofocus>Cancel</button>
          <button
            class="btn btn-primary"
            (click)="store.confirmWorkspaceTrust()"
            [disabled]="store.trustingWorkspace()"
            [attr.aria-busy]="store.trustingWorkspace()"
          >
            Trust folder
          </button>
        </div>
      </pa-dialog>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WorkspaceTrust {
  protected readonly store = inject(AgentStore);
}
