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
const WIDTHS_KEY = 'pwr:card-widths';

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
   * In Focus each card has a width of its own, dragged from its left edge;
   * a card never resized has the one the column had before cards had theirs.
   */
  readonly widths = signal<Partial<Record<CardId, number>>>(loadWidths(this.open(), this.layout.rightWidth()));
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

  /** The column the cards need: as wide as its widest card. */
  readonly columnWidth = computed(() => Math.max(RIGHT.min, ...this.visible().map((card) => this.widthOf(card.id))));

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

  /**
   * A card's width: its own once resized; until then the widest a person
   * gave the cards beside it, so only a card made narrower stands out.
   */
  widthOf(id: CardId): number {
    return this.widths()[id] ?? this.followWidth();
  }

  private readonly followWidth = computed(() => {
    const widths = this.widths();
    const set = this.visible()
      .map((card) => widths[card.id])
      .filter((width): width is number => width !== undefined);
    return set.length ? Math.max(...set) : RIGHT.initial;
  });

  /** The widest a card may be: the conversation keeps its least width beside it. */
  maxWidth(): number {
    const left = this.layout.leftFixed() ?? 0;
    return Math.max(RIGHT.min, this.layout.viewport() - left - MAIN_MIN);
  }

  setWidth(id: CardId, width: number): void {
    const next = Math.round(Math.max(RIGHT.min, Math.min(this.maxWidth(), width)));
    this.widths.update((widths) => ({ ...widths, [id]: next }));
    try {
      localStorage.setItem(WIDTHS_KEY, JSON.stringify(this.widths()));
    } catch {
      // Storage unavailable: the width holds until the app closes.
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
 * The saved widths; the first time, the cards already open take the width
 * the whole column had, so nothing moves.
 */
function loadWidths(open: OpenCard[], column: number): Partial<Record<CardId, number>> {
  const known = new Set<string>(CARDS.map((card) => card.id));
  try {
    const raw = localStorage.getItem(WIDTHS_KEY);
    if (raw) {
      const saved = JSON.parse(raw) as Record<string, unknown>;
      return Object.fromEntries(
        Object.entries(saved).filter(
          ([id, width]) => known.has(id) && typeof width === 'number' && Number.isFinite(width) && width >= RIGHT.min,
        ),
      ) as Partial<Record<CardId, number>>;
    }
  } catch {
    // Unreadable: every card starts at the default width.
  }
  return column === RIGHT.initial ? {} : Object.fromEntries(open.map((card) => [card.id, column]));
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
