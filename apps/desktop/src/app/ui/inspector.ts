import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { ActivityStore } from '../core/activity';
import { AgentStore } from '../core/agent.store';
import { LayoutService, RIGHT } from '../core/layout';
import { SHORTCUTS, roveFocus, shortcut } from '../core/ui';
import { CardId, CardInfo, WorkbenchStore } from '../core/workbench';
import { Icon } from './kit/icon';
import { ResizeHandle } from './kit/resize-handle';
import { Tooltip } from './kit/tooltip';
import { ActivityCard, BrowserCard, FilesCard, PlanCard, ReviewCard, TerminalCard } from './workbench/cards';
import { KnowledgeCard } from './workbench/knowledge';

/**
 * The inspector: one panel on the right, its tools as tabs -- Changes,
 * Terminal, Preview, Files, Checks -- one at a time and full height.
 * Knowledge and Activity join the tabs while the palette has one open.
 */
@Component({
  selector: 'pa-inspector',
  imports: [
    Icon,
    Tooltip,
    ResizeHandle,
    ReviewCard,
    PlanCard,
    ActivityCard,
    FilesCard,
    TerminalCard,
    BrowserCard,
    KnowledgeCard,
  ],
  template: `
    <aside class="inspector" aria-label="Inspector">
      <header class="inspector-head titlebar-row" data-tauri-drag-region="deep">
        <div class="inspector-tabs" role="tablist" aria-label="Tools" (keydown)="tabKeys($event)">
          @for (card of work.tabs(); track card.id) {
            @let selected = work.active() === card.id;
            <span class="inspector-tab-slot" [class.is-selected]="selected">
              <button
                class="inspector-tab"
                role="tab"
                [id]="'inspector-tab-' + card.id"
                aria-controls="inspector-panel"
                [attr.aria-selected]="selected"
                [attr.tabindex]="selected ? 0 : -1"
                (click)="work.show(card.id)"
                [paTooltip]="card.keys ? card.description : null"
                [paTooltipKeys]="card.keys ?? null"
              >
                {{ card.label }}
                @if (badge(card.id); as count) {
                  <span class="inspector-count num">{{ count }}</span>
                }
              </button>
              @if (card.extra) {
                <button class="inspector-tab-close" (click)="work.close(card.id)" [attr.aria-label]="'Close ' + card.label" tabindex="-1">
                  <pa-icon name="x" [size]="12" />
                </button>
              }
            </span>
          }
        </div>
        <span class="spacer" data-tauri-drag-region="deep"></span>
        <button
          class="icon-btn icon-btn-sm"
          (click)="layout.toggleRight()"
          aria-label="Hide the inspector"
          paTooltip="Hide the inspector"
          [paTooltipKeys]="shortcuts.toggleInspector"
        >
          <pa-icon name="panel-right" [size]="16" />
        </button>
      </header>

      <div class="inspector-body" role="tabpanel" id="inspector-panel" [attr.aria-labelledby]="'inspector-tab-' + work.active()">
        @switch (work.active()) {
          @case ('review') { <pa-review-card /> }
          @case ('knowledge') { <pa-knowledge-card /> }
          @case ('files') { <pa-files-card /> }
          @case ('terminal') { <pa-terminal-card /> }
          @case ('browser') { <pa-browser-card /> }
          @case ('activity') { <pa-activity-card /> }
          @case ('plan') { <pa-plan-card /> }
        }
      </div>
    </aside>
    @if (layout.right() === 'docked') {
      <pa-resize-handle
        edge="left"
        label="Resize the inspector"
        [width]="layout.rightWidth()"
        [min]="bounds.min"
        [max]="bounds.max"
        [initial]="bounds.initial"
        (resize)="layout.setRightWidth($event)"
      />
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Inspector {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly work = inject(WorkbenchStore);
  private readonly activity = inject(ActivityStore);
  protected readonly shortcuts = SHORTCUTS;
  protected readonly bounds = RIGHT;
  protected readonly keys = shortcut;

  private readonly badges = computed<Partial<Record<CardId, number>>>(() => ({
    review: this.store.changes().length,
    activity: this.activity.running(),
  }));

  protected badge(id: CardInfo['id']): number {
    return this.badges()[id] ?? 0;
  }

  /** The arrow keys move along the tabs, and show the one they land on. */
  protected tabKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=tab]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }
}
