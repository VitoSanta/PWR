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
 * Review, Terminal, Web preview, Files, Knowledge, Plan & checks, Activity --
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
      @if (!work.focusMode()) {
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
            <div class="menu" role="none">
              @for (card of work.available(); track card.id) {
                <button class="menu-item" role="menuitem" (click)="open(card.id)">
                  <pa-icon [name]="icon(card)" [size]="16" />
                  <span class="truncate">{{ card.label }}</span>
                  @if (work.isOpen(card.id)) {
                    <pa-icon class="menu-hint tool-menu-open" name="check" [size]="14" />
                  } @else if (card.keys) {
                    <span class="kbd menu-hint">{{ keys(card.keys) }}</span>
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
      }

      <div
        class="workbench-body"
        [class.maximized]="!!work.focused()"
        [class.cols-2]="work.focusMode() && work.grid() === 2"
      >
        @for (card of work.visible(); track card.id; let first = $first; let last = $last; let index = $index) {
          <section
            class="wb-card"
            [class.collapsed]="card.collapsed"
            [class.fills]="!card.collapsed"
            [attr.aria-label]="info(card.id).label"
            [style.flex-grow]="card.collapsed ? 0 : cardWeight(card.id)"
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
                @if (work.focusMode() && !work.focused() && work.visible().length > 1) {
                  <button
                    class="icon-btn icon-btn-sm"
                    (click)="work.setColumns(work.columns() === 2 ? 1 : 2)"
                    [attr.aria-label]="work.columns() === 2 ? 'Stack the tools' : 'Tools side by side'"
                    [paTooltip]="work.columns() === 2 ? 'Stack the tools' : 'Tools side by side'"
                  >
                    <pa-icon [name]="work.columns() === 2 ? 'rows' : 'columns'" [size]="14" />
                  </button>
                }
                @if (!work.focused() && work.visible().length > 1) {
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
                    [attr.aria-label]="work.focused() === card.id ? 'Restore' : 'Maximise'"
                    [paTooltip]="work.focused() === card.id ? (work.focusMode() ? 'Leave full screen · Esc' : 'Show the other cards') : (work.focusMode() ? 'Full screen' : 'Fill the column')"
                  >
                    <pa-icon [name]="work.focused() === card.id ? 'minimize' : 'maximize'" [size]="14" />
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
          @if (work.focusMode() && work.grid() === 2) {
            <!-- Side by side: the grid's own gap separates the cards. -->
          } @else if (work.focusMode() && !last && !card.collapsed && !work.visible()[index + 1].collapsed) {
            <div
              class="wb-splitter"
              role="separator"
              tabindex="0"
              aria-orientation="horizontal"
              [attr.aria-label]="'Resize ' + info(card.id).label + ' and ' + info(work.visible()[index + 1].id).label"
              (pointerdown)="startCardResize($event, card.id, work.visible()[index + 1].id)"
              (pointermove)="moveCardResize($event)"
              (pointerup)="endCardResize($event)"
              (pointercancel)="endCardResize($event)"
              (lostpointercapture)="endCardResize($event)"
              (keydown)="keyCardResize($event, card.id, work.visible()[index + 1].id)"
            ></div>
          } @else if (work.focusMode() && !last) {
            <div class="wb-card-gap" aria-hidden="true"></div>
          }
        } @empty {
          <div class="launcher" role="list" aria-label="Tools" animate.enter="anim-fade-in">
            @for (card of work.available(); track card.id) {
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
    @if (work.focusMode() && layout.right() === 'docked' && !work.focused()) {
      <!-- One edge for all the tools: every card follows the width it sets. -->
      <pa-resize-handle
        edge="left"
        [offset]="0"
        label="Resize the tools"
        [width]="work.columnWidth()"
        [min]="work.minWidth()"
        [max]="work.maxWidth()"
        [initial]="bounds.initial"
        (resize)="work.setWidth($event)"
      />
    }
    @if (layout.right() === 'docked' && !work.focusMode()) {
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
  protected readonly launcher = signal(false);
  protected readonly keys = shortcut;
  private readonly cardWeights = signal<Partial<Record<CardId, number>>>(this.loadCardWeights());
  private resizeOrigin: {
    pointerId: number;
    y: number;
    before: CardId;
    after: CardId;
    beforeHeight: number;
    totalHeight: number;
    totalWeight: number;
  } | null = null;

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

  protected cardWeight(id: CardId): number {
    return this.cardWeights()[id] ?? 1;
  }

  protected startCardResize(event: PointerEvent, before: CardId, after: CardId): void {
    if (event.button !== 0) return;
    const separator = event.currentTarget as HTMLElement;
    const first = separator.previousElementSibling as HTMLElement | null;
    const second = separator.nextElementSibling as HTMLElement | null;
    if (!first || !second) return;
    const totalHeight = first.offsetHeight + second.offsetHeight;
    if (totalHeight < 320) return;
    this.resizeOrigin = {
      pointerId: event.pointerId,
      y: event.clientY,
      before,
      after,
      beforeHeight: first.offsetHeight,
      totalHeight,
      totalWeight: this.cardWeight(before) + this.cardWeight(after),
    };
    separator.setPointerCapture(event.pointerId);
    this.layout.resizing.set(true);
    event.preventDefault();
  }

  protected moveCardResize(event: PointerEvent): void {
    const origin = this.resizeOrigin;
    if (!origin || origin.pointerId !== event.pointerId) return;
    this.setCardSplit(origin, origin.beforeHeight + event.clientY - origin.y);
  }

  protected endCardResize(event: PointerEvent): void {
    if (this.resizeOrigin?.pointerId !== event.pointerId) return;
    this.resizeOrigin = null;
    this.layout.resizing.set(false);
    this.saveCardWeights();
  }

  protected keyCardResize(event: KeyboardEvent, before: CardId, after: CardId): void {
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
    const separator = event.currentTarget as HTMLElement;
    const first = separator.previousElementSibling as HTMLElement | null;
    const second = separator.nextElementSibling as HTMLElement | null;
    if (!first || !second) return;
    const totalHeight = first.offsetHeight + second.offsetHeight;
    if (totalHeight < 320) return;
    const origin = {
      pointerId: -1,
      y: 0,
      before,
      after,
      beforeHeight: first.offsetHeight,
      totalHeight,
      totalWeight: this.cardWeight(before) + this.cardWeight(after),
    };
    this.setCardSplit(origin, first.offsetHeight + (event.key === 'ArrowDown' ? 32 : -32));
    this.saveCardWeights();
    event.preventDefault();
  }

  private setCardSplit(origin: NonNullable<Inspector['resizeOrigin']>, desiredHeight: number): void {
    const beforeHeight = Math.max(160, Math.min(origin.totalHeight - 160, desiredHeight));
    const beforeWeight = origin.totalWeight * beforeHeight / origin.totalHeight;
    this.cardWeights.update((weights) => ({
      ...weights,
      [origin.before]: beforeWeight,
      [origin.after]: origin.totalWeight - beforeWeight,
    }));
  }

  private loadCardWeights(): Partial<Record<CardId, number>> {
    try {
      const parsed = JSON.parse(localStorage.getItem('pwr:card-weights') ?? '{}') as Partial<Record<CardId, number>>;
      return Object.fromEntries(Object.entries(parsed).filter(([, weight]) => typeof weight === 'number' && weight > 0));
    } catch {
      return {};
    }
  }

  private saveCardWeights(): void {
    try {
      localStorage.setItem('pwr:card-weights', JSON.stringify(this.cardWeights()));
    } catch {
      // Keep the current sizes when storage is unavailable.
    }
  }

  protected open(id: CardId): void {
    this.launcher.set(false);
    this.work.show(id);
  }
}
