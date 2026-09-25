import { ApplicationRef, Injectable, computed, inject, signal } from '@angular/core';
import { LayoutService } from './layout';

/** The tools the workbench can show, each as a card. */
export type CardId = 'review' | 'knowledge' | 'files' | 'terminal' | 'browser' | 'activity' | 'plan';

export interface CardInfo {
  id: CardId;
  label: string;
  icon: string;
  description: string;
  /** A shortcut, as `SHORTCUTS` spells them. */
  keys?: string;
}

/** In the order the launcher offers them. */
export const CARDS: CardInfo[] = [
  { id: 'review', label: 'Review', icon: 'git-compare', description: 'What PWR changed, file by file', keys: 'Ctrl+Shift+G' },
  { id: 'terminal', label: 'Terminal', icon: 'terminal', description: 'Your shell, in this workspace', keys: 'Ctrl+`' },
  { id: 'browser', label: 'Browser', icon: 'globe', description: 'Preview the app on localhost', keys: 'Mod+Shift+T' },
  { id: 'files', label: 'Files', icon: 'folder', description: 'Browse and read the workspace', keys: 'Mod+P' },
  { id: 'knowledge', label: 'Knowledge', icon: 'target', description: 'The project as a graph, with what was done' },
  { id: 'plan', label: 'Plan & checks', icon: 'shield-check', description: 'Verify, report, diagnose' },
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

/**
 * The right-hand column: tools as cards, stacked, each collapsible,
 * maximisable and closable. Replaces the inspector's fixed tabs; what is open
 * is remembered across launches.
 */
@Injectable({ providedIn: 'root' })
export class WorkbenchStore {
  private readonly layout = inject(LayoutService);
  private readonly app = inject(ApplicationRef);
  private readonly saved = load();

  readonly open = signal<OpenCard[]>(this.saved.open);
  readonly maximized = signal<CardId | null>(this.saved.maximized);
  /** The file the Files card should show, when another card asks for one. */
  readonly fileRequest = signal<string | null>(null);

  readonly visible = computed(() => {
    const maximized = this.maximized();
    const open = this.open();
    return maximized ? open.filter((card) => card.id === maximized) : open;
  });

  isOpen(id: CardId): boolean {
    return this.open().some((card) => card.id === id);
  }

  /** Opens a card, or brings it back if collapsed, and shows the column. */
  show(id: CardId): void {
    this.animate(() => this.showNow(id));
  }

  private showNow(id: CardId): void {
    this.open.update((open) =>
      open.some((card) => card.id === id)
        ? open.map((card) => (card.id === id ? { ...card, collapsed: false } : card))
        : [...open, { id, collapsed: false }],
    );
    if (this.maximized() && this.maximized() !== id) this.maximized.set(null);
    if (this.layout.right() === 'hidden') this.layout.toggleRight();
    this.persist();
  }

  /** A shortcut's toggle: shows a card, or closes it when it is showing. */
  toggle(id: CardId): void {
    const card = this.open().find((item) => item.id === id);
    if (card && !card.collapsed && this.layout.right() !== 'hidden') this.close(id);
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
