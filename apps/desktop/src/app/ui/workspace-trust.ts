import { ChangeDetectionStrategy, Component, HostListener, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';

@Component({
  selector: 'pa-workspace-trust',
  template: `
    @if (store.pendingWorkspaceTrust(); as folder) {
      <div class="scrim">
        <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="trust-title" aria-describedby="trust-description">
          <span class="shield" aria-hidden="true">⛨</span>
          <h2 id="trust-title">Trust this folder?</h2>
          <p class="asked trust-path">{{ folder }}</p>
          <p id="trust-description" class="muted">PWR can read and edit files and run commands here. Review this folder and its project instructions before continuing. Your Ask or Auto-approve setting stays as configured.</p>
          @if (store.workspaceTrustError()) { <p class="warn">{{ store.workspaceTrustError() }}</p> }
          <div class="choices trust-choices">
            <button class="ghost" (click)="store.cancelWorkspaceTrust()" [disabled]="store.trustingWorkspace()">Cancel</button>
            <button class="primary" (click)="store.confirmWorkspaceTrust()" [disabled]="store.trustingWorkspace()">{{ store.trustingWorkspace() ? 'Opening…' : 'Trust folder' }}</button>
          </div>
        </div>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WorkspaceTrust {
  protected readonly store = inject(AgentStore);

  @HostListener('document:keydown.escape')
  protected cancel(): void {
    this.store.cancelWorkspaceTrust();
  }
}
