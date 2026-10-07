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
}

interface Saved {
  open: OpenCard[];
  maximized: CardId | null;
}

const KEY = 'pwr:workbench';
const WIDTH_KEY = 'pwr:tools-width';
const LEGACY_WIDTHS_KEY = 'pwr:card-widths';
const COLUMNS_KEY = 'pwr:tools-columns';
/** What two columns of tools start at: room for a diff beside a terminal. */
const TWO_COLUMNS = 760;

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
   * In Focus the tools are one grid with one width, dragged from its left
   * edge: every card follows it. They used to have a width each, which left
   * a column of cards with ragged left edges.
   */
  readonly width = signal<number>(loadWidth(this.layout.rightWidth()));
  /** How many columns the person asked for: tools stacked, or side by side. */
  readonly columns = signal<1 | 2>(loadColumns());
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

  /**
   * The columns in use: never more than there are cards, so one tool has the
   * whole height, two stand side by side or one above the other, four make
   * two rows of two and six three.
   */
  readonly grid = computed<1 | 2>(() => (this.columns() === 2 && this.visible().length > 1 ? 2 : 1));

  /** The least the tools take: each column keeps a card's least width. */
  readonly minWidth = computed(() => RIGHT.min * this.grid());

  /** The width of the whole grid. */
  readonly columnWidth = computed(() => Math.max(this.minWidth(), this.width()));

  constructor() {
    // Focus docks the column by its widest card, so the conversation keeps
    // the rest of the window, centred in it. In a narrower window the wide
    // cards give way first, down to the least a card takes; only then do
    // the tools take the page.
    effect(() => {
      if (!this.focusMode()) return;
      const width = Math.min(this.columnWidth(), this.maxWidth());
      untracked(() => this.layout.rightWidth.set(width));
    });
  }


  /** The widest a card may be: the conversation keeps its least width beside it. */
  maxWidth(): number {
    const left = this.layout.leftFixed() ?? 0;
    return Math.max(RIGHT.min, this.layout.viewport() - left - MAIN_MIN);
  }

  /** Resizes the whole grid: every card follows. */
  setWidth(width: number): void {
    this.width.set(Math.round(Math.max(this.minWidth(), Math.min(this.maxWidth(), width))));
    try {
      localStorage.setItem(WIDTH_KEY, String(this.width()));
    } catch {
      // Storage unavailable: the width holds until the app closes.
    }
  }

  /** Stacks the tools in one column, or stands them side by side in two. */
  setColumns(columns: 1 | 2): void {
    this.animate(() => {
      const before = this.columns();
      this.columns.set(columns);
      // Two columns in the room of one would be two slivers; going back, the
      // single column takes one column's share again.
      if (columns === 2 && before === 1) this.setWidth(Math.max(this.width(), TWO_COLUMNS));
      if (columns === 1 && before === 2) this.setWidth(Math.max(RIGHT.initial, Math.round(this.width() / 2)));
      try {
        localStorage.setItem(COLUMNS_KEY, String(columns));
      } catch {
        // Storage unavailable: the choice holds until the app closes.
      }
    });
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
    this.open.update((open) =>
      open.some((card) => card.id === id)
        ? open.map((card) => (card.id === id ? { ...card, collapsed: false } : card))
        : [...open, { id, collapsed: false }],
    );
    if (this.maximized() && this.maximized() !== id) this.maximized.set(null);
    if (!this.panelVisible()) this.layout.toggleRight();
    this.persist();
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
    const transition = start.call(document, () => {
      change();
      // The new layout has to be on the page before the transition's
      // second snapshot is taken.
      this.app.tick();
    }) as { ready?: Promise<unknown>; finished?: Promise<unknown> } | undefined;
    // A transition the browser gives up on -- the window hidden, another one
    // started over it -- has still made its change; only the motion is lost.
    transition?.ready?.catch(() => undefined);
    transition?.finished?.catch(() => undefined);
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
 * The saved width of the tools; the first time, the widest the cards had
 * when each had its own, or the width the column had before that.
 */
function loadWidth(column: number): number {
  try {
    const saved = Number(localStorage.getItem(WIDTH_KEY));
    if (Number.isFinite(saved) && saved >= RIGHT.min) return saved;
    const each = JSON.parse(localStorage.getItem(LEGACY_WIDTHS_KEY) ?? '{}') as Record<string, unknown>;
    const widths = Object.values(each).filter(
      (width): width is number => typeof width === 'number' && Number.isFinite(width) && width >= RIGHT.min,
    );
    if (widths.length) return Math.max(...widths);
  } catch {
    // Unreadable: the tools start at the column's width.
  }
  return column;
}

function loadColumns(): 1 | 2 {
  try {
    return localStorage.getItem(COLUMNS_KEY) === '2' ? 2 : 1;
  } catch {
    return 1;
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
