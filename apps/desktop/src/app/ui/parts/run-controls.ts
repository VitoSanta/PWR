import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { Popover } from '../kit/popover';

/**
 * Goal mode and Auto-approve, anchored to whatever asked for them: the
 * composer's note that one is on, or the Tools row.
 */
@Component({
  selector: 'pa-run-controls',
  imports: [Popover],
  template: `
    <pa-popover
      class="focus-run-popover"
      [anchor]="anchor()"
      [side]="side()"
      anchorAlign="end"
      width="300px"
      ariaLabel="Run controls"
      (closed)="closed.emit()"
      animate.leave="anim-pop-out"
    >
      <div class="popover-head"><h2 class="popover-title">Run controls</h2></div>
      <div class="focus-run-options">
        <button
          class="focus-run-option"
          [attr.aria-pressed]="store.goalMode()"
          (click)="store.goalMode.set(!store.goalMode())"
        >
          <span class="focus-run-copy">
            <strong>Goal mode</strong>
            <small>Keep working until the goal is verified.</small>
          </span>
          <span class="switch" aria-hidden="true"></span>
        </button>
        <button
          class="focus-run-option tone-warning"
          [attr.aria-pressed]="store.permissionMode() === 'auto'"
          (click)="store.setPermissionMode(store.permissionMode() === 'auto' ? 'ask' : 'auto')"
        >
          <span class="focus-run-copy">
            <strong>Auto-approve</strong>
            <small>{{
              store.permissionMode() === 'auto'
                ? 'Permissions are granted automatically.'
                : 'PWR asks before sensitive actions.'
            }}</small>
          </span>
          <span class="switch" aria-hidden="true"></span>
        </button>
      </div>
    </pa-popover>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RunControls {
  protected readonly store = inject(AgentStore);
  readonly anchor = input.required<HTMLElement>();
  readonly closed = output<void>();

  /** Up from the composer at the bottom of the window, down from anything above. */
  protected side(): 'top' | 'bottom' {
    return this.anchor().getBoundingClientRect().top > window.innerHeight / 2 ? 'top' : 'bottom';
  }
}
