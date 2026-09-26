import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { LayoutService } from '../../core/layout';
import { CardInfo, WorkbenchStore } from '../../core/workbench';
import { Icon, IconName } from '../kit/icon';
import { Tooltip } from '../kit/tooltip';

/**
 * The workbench's tools as a row or a column of buttons: each opens its card,
 * or closes it when it is showing. Lit while its card is open.
 */
@Component({
  selector: 'pa-tool-dock',
  imports: [Icon, Tooltip],
  template: `
    <div class="tool-dock" [class.is-vertical]="vertical()" role="toolbar" aria-label="Tools">
      @for (card of work.available(); track card.id) {
        <button
          class="icon-btn"
          [class.icon-btn-sm]="small()"
          [attr.aria-pressed]="showing(card)"
          (click)="work.toggle(card.id)"
          [attr.aria-label]="card.label"
          [paTooltip]="card.label"
          [paTooltipKeys]="card.keys ?? null"
        >
          <pa-icon [name]="icon(card)" [size]="small() ? 15 : 18" />
        </button>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolDock {
  protected readonly work = inject(WorkbenchStore);
  private readonly layout = inject(LayoutService);

  readonly vertical = input(false);
  readonly small = input(false);

  protected showing(card: CardInfo): boolean {
    return this.layout.right() !== 'hidden' && this.work.isOpen(card.id);
  }

  protected icon(card: CardInfo): IconName {
    return card.icon as IconName;
  }
}
