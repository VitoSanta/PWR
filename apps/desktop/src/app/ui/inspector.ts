import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { ActivityStore } from '../core/activity';
import { AgentStore } from '../core/agent.store';
import { LayoutService, RIGHT } from '../core/layout';
import { SHORTCUTS, shortcut } from '../core/ui';
import { CARDS, CardId, CardInfo, WorkbenchStore } from '../core/workbench';
import { Icon, IconName } from './kit/icon';
import { Popover } from './kit/popover';
import { ResizeHandle } from './kit/resize-handle';
import { Tooltip } from './kit/tooltip';
import { ActivityCard, BrowserCard, FilesCard, PlanCard, ReviewCard, TerminalCard } from './workbench/cards';
import { KnowledgeCard } from './workbench/knowledge';

/**
 * The workbench: the right-hand column, where the tools live as cards --
 * Review, Terminal, Browser, Files, Knowledge, Plan & checks, Activity --
 * stacked, each collapsible, movable, maximisable and closable.
 */
@Component({
  selector: 'pa-inspector',
  imports: [
    Icon,
    Popover,
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
    <aside class="workbench" aria-label="Workbench">
      <header class="workbench-head titlebar-row" data-tauri-drag-region="deep">
        <span class="workbench-title">Workbench</span>
        <span class="spacer" data-tauri-drag-region="deep"></span>
        <button
          #addTrigger
          class="icon-btn icon-btn-sm"
          (click)="launcher.set(!launcher())"
          aria-label="Open a tool"
          aria-haspopup="menu"
          [attr.aria-expanded]="launcher()"
          paTooltip="Open a tool"
        >
          <pa-icon name="plus" [size]="16" />
        </button>
        @if (launcher()) {
          <pa-popover [anchor]="addTrigger" anchorAlign="end" width="300px" ariaLabel="Tools" panelRole="menu" [focusFirst]="true" (closed)="launcher.set(false)" animate.leave="anim-pop-out">
            <div class="tool-menu" role="none">
              @for (card of cards; track card.id) {
                <button class="tool-menu-item" role="menuitem" (click)="open(card.id)">
                  <pa-icon [name]="icon(card)" [size]="16" />
                  <span class="tool-menu-label">{{ card.label }}</span>
                  @if (work.isOpen(card.id)) {
                    <pa-icon class="tool-menu-open" name="check" [size]="14" />
                  } @else if (card.keys) {
                    <span class="kbd">{{ keys(card.keys) }}</span>
                  }
                </button>
              }
            </div>
          </pa-popover>
        }
        <button
          class="icon-btn icon-btn-sm"
          (click)="layout.toggleRight()"
          aria-label="Hide the workbench"
          paTooltip="Hide the workbench"
          [paTooltipKeys]="shortcuts.toggleInspector"
        >
          <pa-icon name="x" [size]="16" />
        </button>
      </header>

      <div class="workbench-body" [class.maximized]="!!work.maximized()">
        @for (card of work.visible(); track card.id; let first = $first; let last = $last) {
          <section
            class="wb-card"
            [class.collapsed]="card.collapsed"
            [class.fills]="!card.collapsed"
            [attr.aria-label]="info(card.id).label"
            [style.view-transition-name]="'wb-' + card.id"
          >
            <header class="wb-card-head" (dblclick)="work.maximize(card.id)">
              <button class="wb-card-title" (click)="work.collapse(card.id)" [attr.aria-expanded]="!card.collapsed">
                <pa-icon class="wb-card-chevron" name="chevron-right" [size]="14" />
                <pa-icon [name]="icon(info(card.id))" [size]="15" />
                <span>{{ info(card.id).label }}</span>
                @if (badge(card.id); as count) {
                  <span class="count num">{{ count }}</span>
                }
              </button>
              <span class="spacer"></span>
              <span class="wb-card-actions">
                @if (!work.maximized() && work.visible().length > 1) {
                  <button class="icon-btn icon-btn-sm" (click)="work.move(card.id, -1)" [disabled]="first" aria-label="Move up" paTooltip="Move up">
                    <pa-icon name="chevron-up" [size]="14" />
                  </button>
                  <button class="icon-btn icon-btn-sm" (click)="work.move(card.id, 1)" [disabled]="last" aria-label="Move down" paTooltip="Move down">
                    <pa-icon name="chevron-down" [size]="14" />
                  </button>
                }
                <button
                  class="icon-btn icon-btn-sm"
                  (click)="work.maximize(card.id)"
                  [attr.aria-label]="work.maximized() === card.id ? 'Restore' : 'Maximise'"
                  [paTooltip]="work.maximized() === card.id ? 'Show the other cards' : 'Fill the column'"
                >
                  <pa-icon [name]="work.maximized() === card.id ? 'minus' : 'panel-right'" [size]="14" />
                </button>
                <button class="icon-btn icon-btn-sm" (click)="work.close(card.id)" [attr.aria-label]="'Close ' + info(card.id).label" paTooltip="Close">
                  <pa-icon name="x" [size]="14" />
                </button>
              </span>
            </header>
            @if (!card.collapsed) {
              <div class="wb-card-body" animate.enter="card-body-in">
                @switch (card.id) {
                  @case ('review') { <pa-review-card /> }
                  @case ('knowledge') { <pa-knowledge-card /> }
                  @case ('files') { <pa-files-card /> }
                  @case ('terminal') { <pa-terminal-card /> }
                  @case ('browser') { <pa-browser-card /> }
                  @case ('activity') { <pa-activity-card /> }
                  @case ('plan') { <pa-plan-card /> }
                }
              </div>
            }
          </section>
        } @empty {
          <div class="launcher" role="list" aria-label="Tools" animate.enter="anim-fade-in">
            @for (card of cards; track card.id) {
              <button class="launcher-card" role="listitem" (click)="open(card.id)">
                <pa-icon [name]="icon(card)" [size]="18" />
                <span class="launcher-text">
                  <span class="launcher-label">{{ card.label }}</span>
                  <span class="t-meta">{{ card.description }}</span>
                </span>
                @if (card.keys) {
                  <span class="kbd">{{ keys(card.keys) }}</span>
                }
              </button>
            }
          </div>
        }
      </div>
    </aside>
    @if (layout.right() === 'docked') {
      <pa-resize-handle
        edge="left"
        label="Resize the workbench"
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
  protected readonly cards = CARDS;
  protected readonly launcher = signal(false);
  protected readonly keys = shortcut;

  private readonly byId = new Map(CARDS.map((card) => [card.id, card]));
  private readonly badges = computed<Partial<Record<CardId, number>>>(() => ({
    review: this.store.changes().length,
    activity: this.activity.running(),
  }));

  protected info(id: CardId): CardInfo {
    return this.byId.get(id)!;
  }

  protected icon(card: CardInfo): IconName {
    return card.icon as IconName;
  }

  protected badge(id: CardId): number {
    return this.badges()[id] ?? 0;
  }

  protected open(id: CardId): void {
    this.launcher.set(false);
    this.work.show(id);
  }
}
