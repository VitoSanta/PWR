import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { PERMISSION_MODES } from '../../core/model';
import { Icon } from '../kit/icon';
import { Popover } from '../kit/popover';

/**
 * Goal mode and the permissions mode, anchored to whatever asked for them: the
 * composer's note that one is on, or the Tools row.
 */
@Component({
  selector: 'pa-run-controls',
  imports: [Icon, Popover],
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
      <div class="popover-head">
        <h2 class="popover-title">Run controls</h2>
        <button class="icon-btn icon-btn-sm" (click)="closed.emit()" aria-label="Close">
          <pa-icon name="x" [size]="16" />
        </button>
      </div>
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
      </div>
      <div class="focus-run-options focus-run-modes" role="radiogroup" aria-label="Permissions">
        <span class="focus-run-heading">Permissions</span>
        @for (option of modes; track option.mode) {
          <button
            class="focus-run-option"
            [class.tone-warning]="option.mode === 'full'"
            role="radio"
            [attr.aria-checked]="store.permissionMode() === option.mode"
            [attr.aria-pressed]="store.permissionMode() === option.mode"
            (click)="store.setPermissionMode(option.mode)"
          >
            <span class="focus-run-copy">
              <strong>{{ option.label }}</strong>
              <small>{{ option.summary }}</small>
            </span>
            <span class="focus-run-radio" aria-hidden="true"></span>
          </button>
        }
      </div>
    </pa-popover>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RunControls {
  protected readonly store = inject(AgentStore);
  protected readonly modes = PERMISSION_MODES;
  readonly anchor = input.required<HTMLElement>();
  readonly closed = output<void>();

  /** Up from the composer at the bottom of the window, down from anything above. */
  protected side(): 'top' | 'bottom' {
    return this.anchor().getBoundingClientRect().top > window.innerHeight / 2 ? 'top' : 'bottom';
  }
}
