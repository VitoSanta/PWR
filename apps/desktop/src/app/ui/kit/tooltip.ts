import { Directive, effect, inject, input } from '@angular/core';
import { MatTooltip } from '@angular/material/tooltip';
import { shortcut } from '../../core/ui';

/**
 * `paTooltip="Toggle sidebar" paTooltipKeys="Mod+B"`: Material's plain
 * tooltip, shown on hover (after a delay) and on keyboard focus, with the
 * shortcut after the label. Icon-only controls still carry their own
 * aria-label; the tooltip is for sighted users.
 */
@Directive({
  selector: '[paTooltip]',
  hostDirectives: [MatTooltip],
})
export class Tooltip {
  readonly paTooltip = input<string | null | undefined>('');
  readonly paTooltipKeys = input<string | null>(null);
  private readonly tooltip = inject(MatTooltip);

  constructor() {
    this.tooltip.showDelay = 450;
    this.tooltip.position = 'below';
    effect(() => {
      const text = this.paTooltip() ?? '';
      const keys = this.paTooltipKeys();
      this.tooltip.message = text && keys ? `${text}  ${shortcut(keys)}` : text;
      this.tooltip.disabled = !text;
    });
  }
}
