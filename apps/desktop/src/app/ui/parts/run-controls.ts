import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { PERMISSION_MODES } from '../../core/model';
import { roveFocus } from '../../core/ui';
import { Icon } from '../kit/icon';
import { Popover } from '../kit/popover';

/**
 * How PWR works: Goal mode and the permissions mode, anchored to whatever
 * asked for them -- the composer's Goal or permissions chip.
 */
@Component({
  selector: 'pa-run-controls',
  imports: [Icon, Popover],
  template: `
    <pa-popover
      [anchor]="anchor()"
      [side]="side()"
      anchorAlign="start"
      width="360px"
      ariaLabel="How PWR works"
      (closed)="closed.emit()"
      animate.leave="anim-pop-out"
    >
      <div class="popover-head">
        <h2 class="popover-title">How PWR works</h2>
        <button class="icon-btn icon-btn-sm" (click)="closed.emit()" aria-label="Close">
          <pa-icon name="x" [size]="16" />
        </button>
      </div>
      <div class="run-body">
        <button class="goal-card" role="switch" [attr.aria-checked]="store.goalMode()" (click)="store.goalMode.set(!store.goalMode())">
          <span class="run-copy">
            <strong>Goal mode</strong>
            <small>Keep working until the goal is verified.</small>
          </span>
          <span class="switch" aria-hidden="true"></span>
        </button>
        <div class="run-modes" role="radiogroup" aria-label="Permissions" (keydown)="modeKeys($event)">
          @for (option of modes; track option.mode) {
            @let chosen = store.permissionMode() === option.mode;
            <button
              class="run-mode"
              [class.tone-warning]="option.mode === 'full'"
              role="radio"
              [attr.aria-checked]="chosen"
              [attr.tabindex]="chosen ? 0 : -1"
              (click)="store.setPermissionMode(option.mode)"
            >
              <span class="run-radio" aria-hidden="true"></span>
              <span class="run-copy">
                <strong>{{ option.label }}</strong>
                <small>{{ option.summary }}</small>
              </span>
            </button>
          }
        </div>
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

  /** The arrow keys choose among the modes, as in any radio group. */
  protected modeKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]'))
      (document.activeElement as HTMLElement | null)?.click();
  }

  /** Up from the composer at the bottom of the window, down from anything above. */
  protected side(): 'top' | 'bottom' {
    return this.anchor().getBoundingClientRect().top > window.innerHeight / 2 ? 'top' : 'bottom';
  }
}
