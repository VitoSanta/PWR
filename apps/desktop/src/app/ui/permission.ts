import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';

@Component({
  selector: 'pa-permission',
  template: `
    @if (store.permission(); as asked) {
      <div class="scrim">
        <div class="dialog" role="alertdialog" aria-modal="true">
          <span class="shield">⛨</span>
          <h2>PWR asks for permission</h2>
          <p class="asked">{{ asked.title }}</p>
          @if (asked.approval) { <p class="muted">Approval: {{ asked.approval }}</p> }
          <div class="choices">
            @for (option of asked.options; track option.optionId) {
              <button [class]="option.kind.startsWith('reject') ? 'danger' : option.kind === 'allow_once' ? 'primary' : 'ghost'"
                      (click)="store.answerPermission(option.optionId)">
                {{ option.name }}
              </button>
            }
          </div>
        </div>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Permission {
  protected readonly store = inject(AgentStore);
}
