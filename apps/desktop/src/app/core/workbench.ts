import { ApplicationRef, Injectable, computed, effect, inject, signal, untracked } from '@angular/core';
import { AgentStore } from './agent.store';
import { LayoutService, MAIN_MIN, RIGHT } from './layout';

/** The tools the workbench can show, each as a card. */
export type CardId = 'review' | 'knowledge' | 'files' | 'terminal' | 'browser' | 'activity' | 'plan';

export interface CardInfo {
  id: CardId;
  label: string;
  icon: string;
  description: string;
  /** A shortcut, as `SHORTCUTS` spells them. */
  keys?: string;
  /** About a workspace, so not offered in chat mode, which has none. */
  workspace?: true;
}

/** In the order the launcher offers them. */
export const CARDS: CardInfo[] = [
  { id: 'review', label: 'Review', icon: 'git-compare', description: 'What PWR changed, file by file', keys: 'Ctrl+Shift+G', workspace: true },
  { id: 'terminal', label: 'Terminal', icon: 'terminal', description: 'Your shell, in this workspace', keys: 'Ctrl+`', workspace: true },
  { id: 'browser', label: 'Web preview', icon: 'globe', description: 'The app this machine serves, on localhost', keys: 'Mod+Shift+T' },
  { id: 'files', label: 'Files', icon: 'folder', description: 'Browse and read the workspace', keys: 'Mod+P', workspace: true },
  { id: 'knowledge', label: 'Knowledge', icon: 'target', description: 'The project as a graph, with what was done' },
  { id: 'plan', label: 'Plan & checks', icon: 'shield-check', description: 'Verify, report, diagnose', workspace: true },
  { id: 'activity', label: 'Activity', icon: 'activity', description: 'Background work and the core log' },
];

interface OpenCard {
  id: CardId;
  collapsed: boolean;
  /** Across the whole column in Focus, rather than one of its two. */
  wide?: boolean;
}

/** Between two cards side by side, as between two above each other. */
export const CARD_GAP = 14;

interface Saved {
  open: OpenCard[];
  maximized: CardId | null;
}

const KEY = 'pwr:workbench';
const GRID_KEY = 'pwr:grid-width';
const SPLITS_KEY = 'pwr:card-splits';

/**
 * The right-hand column: tools as cards, stacked, each collapsible,
 * maximisable and closable. Replaces the inspector's fixed tabs; what is open
 * is remembered across launches.
 */
@Injectable({ providedIn: 'root' })
export class WorkbenchStore {
  private readonly layout = inject(LayoutService);
  private readonly agent = inject(AgentStore);
  private readonly app = inject(ApplicationRef);
  private readonly saved = load();

  readonly open = signal<OpenCard[]>(this.saved.open);
  readonly maximized = signal<CardId | null>(this.saved.maximized);
  /** Focus presents tools as standalone cards without the Workbench frame. */
  readonly focusMode = signal(false);
  /**
   * Focus lays the tools out as widgets on a grid as wide as this: every row
   * takes it all, a pair shares it. Dragged from a card's outer edge.
   */
  readonly gridWidth = signal(loadGrid(this.layout.rightWidth()));
  /** Where each pair divides its row: the left card's share, by its id. */
  readonly splits = signal<Partial<Record<CardId, number>>>(loadSplits());
  /** The file the Files card should show, when another card asks for one. */
  readonly fileRequest = signal<string | null>(null);

  /**
   * The cards this conversation can use: in chat mode, only those that need
   * no workspace. The others stay where they were, for the next workspace.
   */
  readonly available = computed(() => (this.agent.chatMode() ? CARDS.filter((card) => !card.workspace) : CARDS));

  /** The maximised card, when this conversation can use it. */
  readonly focused = computed(() => {
    const maximized = this.maximized();
    return maximized && this.available().some((card) => card.id === maximized) ? maximized : null;
  });

  readonly visible = computed(() => {
    const usable = new Set(this.available().map((card) => card.id));
    const focused = this.focused();
    const open = this.open().filter((card) => usable.has(card.id));
    return focused ? open.filter((card) => card.id === focused) : open;
  });

  /** A Focus tool remains available when a narrow window switches to one pane. */
  readonly panelVisible = computed(() =>
    this.focusMode()
      ? this.layout.rightOpen() || this.layout.rightPeek()
      : this.layout.right() !== 'hidden',
  );

  /** The grid as it shows: never so wide the conversation loses its least width. */
  readonly columnWidth = computed(() => Math.round(Math.max(RIGHT.min, Math.min(this.gridWidth(), this.maxWidth()))));

  /**
   * The cards in rows. In Focus two cards sit side by side when both are
   * one column's worth -- not wide, not collapsed -- and the grid is wide
   * enough for two at a card's least; otherwise each has a row of its own.
   * Elsewhere, and while one is maximised, one card per row.
   */
  readonly rows = computed<CardId[][]>(() => {
    const cards = this.visible();
    const pairs = this.focusMode() && !this.focused() && this.columnWidth() - CARD_GAP >= 2 * RIGHT.min;
    const half = (card: OpenCard | undefined): card is OpenCard => !!card && !card.collapsed && !card.wide;
    const rows: CardId[][] = [];
    for (let index = 0; index < cards.length; index++) {
      const card = cards[index];
      const next = cards[index + 1];
      if (pairs && half(card) && half(next)) {
        rows.push([card.id, next.id]);
        index++;
      } else rows.push([card.id]);
    }
    return rows;
  });

  constructor() {
    // Focus docks the column at the grid's width, so the conversation keeps
    // the rest of the window, centred in it. In a narrower window the grid
    // gives way first, down to a card's least; only then do the tools take
    // the page.
    effect(() => {
      if (!this.focusMode()) return;
      const width = this.columnWidth();
      untracked(() => this.layout.rightWidth.set(width));
    });
  }

  /** The card beside this one in its row, if any. */
  partner(id: CardId): CardId | null {
    const row = this.rows().find((ids) => ids.includes(id));
    return row && row.length > 1 ? (row[0] === id ? row[1] : row[0]) : null;
  }

  isWide(id: CardId): boolean {
    return !!this.open().find((card) => card.id === id)?.wide;
  }

  /** How wide a card shows: the grid's width, or its share of a pair's. */
  widthOf(id: CardId): number {
    const row = this.rows().find((ids) => ids.includes(id));
    const grid = this.columnWidth();
    if (!row || row.length === 1) return grid;
    const room = grid - CARD_GAP;
    const left = Math.round(Math.max(RIGHT.min, Math.min(room - RIGHT.min, room * (this.splits()[row[0]] ?? 0.5))));
    return row[0] === id ? left : room - left;
  }

  /** The widest a card may be: the conversation keeps its least width beside it. */
  maxWidth(): number {
    const left = this.layout.leftFixed() ?? 0;
    return Math.max(RIGHT.min, this.layout.viewport() - left - MAIN_MIN);
  }

  /** How far a card's edge can go: a pair's inner edge until the other card is at its least. */
  maxWidthOf(id: CardId): number {
    const row = this.rows().find((ids) => ids.includes(id));
    if (row?.length === 2 && row[1] === id) return this.columnWidth() - CARD_GAP - RIGHT.min;
    if (row?.length === 2) return this.maxWidth() - CARD_GAP - this.widthOf(row[1]);
    return this.maxWidth();
  }

  /**
   * A card's left edge dragged, as with widgets. Between two cards side by
   * side it moves the line between them: one grows as the other shrinks,
   * and the grid stays. Anywhere else it is the grid's outer edge: the
   * whole grid grows or shrinks, and every row with it, each pair keeping
   * its right card's width while the left one takes the difference.
   */
  resize(id: CardId, width: number): void {
    const row = this.rows().find((ids) => ids.includes(id));
    if (row?.length === 2 && row[1] === id) {
      const room = this.columnWidth() - CARD_GAP;
      const right = Math.max(RIGHT.min, Math.min(room - RIGHT.min, width));
      this.splits.update((splits) => ({ ...splits, [row[0]]: (room - right) / room }));
      this.save(SPLITS_KEY, this.splits());
      return;
    }
    const grid = row?.length === 2 ? width + CARD_GAP + this.widthOf(row[1]) : width;
    this.setGrid(grid);
  }

  /** The grid's width; each pair keeps its right card as it is, where it can. */
  setGrid(width: number): void {
    const next = Math.round(Math.max(RIGHT.min, Math.min(this.maxWidth(), width)));
    const splits = { ...this.splits() };
    for (const row of this.rows()) {
      if (row.length !== 2) continue;
      const room = next - CARD_GAP;
      if (room < 2 * RIGHT.min) continue;
      const right = Math.max(RIGHT.min, Math.min(room - RIGHT.min, this.widthOf(row[1])));
      splits[row[0]] = (room - right) / room;
    }
    this.splits.set(splits);
    this.gridWidth.set(next);
    this.save(SPLITS_KEY, splits);
    this.save(GRID_KEY, next);
  }

  /** One column, or across both. */
  toggleWide(id: CardId): void {
    this.animate(() => {
      this.open.update((open) => open.map((card) => (card.id === id ? { ...card, wide: !card.wide || undefined } : card)));
      this.persist();
    });
  }

  private save(key: string, value: unknown): void {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      // Storage unavailable: the layout holds until the app closes.
    }
  }

  isOpen(id: CardId): boolean {
    return this.visible().some((card) => card.id === id);
  }

  private usable(id: CardId): boolean {
    return this.available().some((card) => card.id === id);
  }

  /** Opens a card, or brings it back if collapsed, and shows the column. */
  show(id: CardId): void {
    if (!this.usable(id)) return;
    this.animate(() => this.showNow(id));
  }

  private showNow(id: CardId): void {
    this.fitBeside(id);
    this.open.update((open) =>
      open.some((card) => card.id === id)
        ? open.map((card) => (card.id === id ? { ...card, collapsed: false } : card))
        : [...open, { id, collapsed: false }],
    );
    if (this.maximized() && this.maximized() !== id) this.maximized.set(null);
    if (!this.panelVisible()) this.layout.toggleRight();
    this.persist();
  }

  /**
   * A card opening after one that has its row to itself goes beside it:
   * if the grid is too narrow for two, it widens to two cards' width, as
   * far as the window allows. Where it cannot, the new card goes below.
   */
  private fitBeside(id: CardId): void {
    if (!this.focusMode() || this.open().some((card) => card.id === id)) return;
    const last = this.visible().at(-1);
    if (!last || last.collapsed || last.wide || this.partner(last.id)) return;
    const two = 2 * RIGHT.min + CARD_GAP;
    if (this.columnWidth() >= two || this.maxWidth() < two) return;
    this.gridWidth.set(Math.min(this.maxWidth(), 2 * RIGHT.initial + CARD_GAP));
    this.save(GRID_KEY, this.gridWidth());
  }

  /** A shortcut's toggle: shows a card, or closes it when it is showing. */
  toggle(id: CardId): void {
    if (!this.usable(id)) return;
    const card = this.open().find((item) => item.id === id);
    if (card && !card.collapsed && this.panelVisible()) this.close(id);
    else this.show(id);
  }

  close(id: CardId): void {
    this.animate(() => this.closeNow(id));
  }

  private closeNow(id: CardId): void {
    this.open.update((open) => open.filter((card) => card.id !== id));
    if (this.maximized() === id) this.maximized.set(null);
    this.persist();
  }

  collapse(id: CardId): void {
    this.animate(() => this.collapseNow(id));
  }

  private collapseNow(id: CardId): void {
    this.open.update((open) => open.map((card) => (card.id === id ? { ...card, collapsed: !card.collapsed } : card)));
    this.persist();
  }

  maximize(id: CardId): void {
    this.animate(() => this.maximizeNow(id));
  }

  private maximizeNow(id: CardId): void {
    this.maximized.set(this.maximized() === id ? null : id);
    this.open.update((open) => open.map((card) => (card.id === id ? { ...card, collapsed: false } : card)));
    this.persist();
  }

  /** Moves a card up (-1) or down (+1) the column. */
  move(id: CardId, by: -1 | 1): void {
    this.animate(() => this.moveNow(id, by));
  }

  private moveNow(id: CardId, by: -1 | 1): void {
    this.open.update((open) => {
      const index = open.findIndex((card) => card.id === id);
      const target = index + by;
      if (index < 0 || target < 0 || target >= open.length) return open;
      const copy = open.slice();
      [copy[index], copy[target]] = [copy[target], copy[index]];
      return copy;
    });
    this.persist();
  }

  /** Opens the Files card on one file. */
  openFile(path: string): void {
    this.fileRequest.set(path);
    this.show('files');
  }

  /**
   * A change to the column, shown as motion: each card slides and resizes
   * to where it goes (it carries a view-transition name), instead of the
   * others jumping when one opens, closes or moves. Where the platform has
   * no view transitions, or the person asked for less motion, it is
   * instant.
   */
  private animate(change: () => void): void {
    const start = (document as Document & { startViewTransition?: (update: () => void) => unknown })
      .startViewTransition;
    const still = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!start || still) {
      change();
      return;
    }
    start.call(document, () => {
      change();
      // The new layout has to be on the page before the transition's
      // second snapshot is taken.
      this.app.tick();
    });
  }

  private persist(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify({ open: this.open(), maximized: this.maximized() }));
    } catch {
      // A private window or blocked storage: the layout just is not kept.
    }
  }
}

/**
 * The grid's saved width; the first time, the widest a card had been given
 * when each had its own, or the column's width, so nothing jumps.
 */
function loadGrid(column: number): number {
  try {
    const grid = Number(JSON.parse(localStorage.getItem(GRID_KEY) ?? 'null'));
    if (Number.isFinite(grid) && grid >= RIGHT.min) return grid;
    const old = JSON.parse(localStorage.getItem('pwr:card-widths') ?? '{}') as Record<string, unknown>;
    const widths = Object.values(old).filter((width): width is number => typeof width === 'number' && width >= RIGHT.min);
    if (widths.length) return Math.max(...widths);
  } catch {
    // Unreadable: the column's width.
  }
  return Math.max(RIGHT.min, column);
}

function loadSplits(): Partial<Record<CardId, number>> {
  const known = new Set<string>(CARDS.map((card) => card.id));
  try {
    const saved = JSON.parse(localStorage.getItem(SPLITS_KEY) ?? '{}') as Record<string, unknown>;
    return Object.fromEntries(
      Object.entries(saved).filter(([id, share]) => known.has(id) && typeof share === 'number' && share > 0 && share < 1),
    ) as Partial<Record<CardId, number>>;
  } catch {
    return {};
  }
}

function load(): Saved {
  const fallback: Saved = { open: [{ id: 'review', collapsed: false }], maximized: null };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return fallback;
    const saved = JSON.parse(raw) as Saved;
    const known = new Set(CARDS.map((card) => card.id));
    const open = (saved.open ?? []).filter((card) => known.has(card.id));
    const maximized = saved.maximized && known.has(saved.maximized) ? saved.maximized : null;
    return { open, maximized };
  } catch {
    return fallback;
  }
}
