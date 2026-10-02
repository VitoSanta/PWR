import { Injectable, computed, effect, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import { LayoutService } from './layout';

/** The tools the inspector can show, one at a time. */
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
  /**
   * Not one of the inspector's tabs: reached from the command palette, and
   * shown as a tab of its own only while it is open.
   */
  extra?: true;
}

/** In the order the inspector's tabs show them. */
export const CARDS: CardInfo[] = [
  { id: 'review', label: 'Changes', icon: 'git-compare', description: 'What PWR changed, file by file', keys: 'Ctrl+Shift+G', workspace: true },
  { id: 'terminal', label: 'Terminal', icon: 'terminal', description: 'Your shell, in this workspace', keys: 'Ctrl+`', workspace: true },
  { id: 'browser', label: 'Preview', icon: 'globe', description: 'The app this machine serves, on localhost', keys: 'Mod+Shift+T' },
  { id: 'files', label: 'Files', icon: 'folder', description: 'Browse and read the workspace', keys: 'Mod+P', workspace: true },
  { id: 'plan', label: 'Checks', icon: 'shield-check', description: 'Verify, report, diagnose', workspace: true },
  { id: 'knowledge', label: 'Knowledge', icon: 'target', description: 'The project as a graph, with what was done', extra: true },
  { id: 'activity', label: 'Activity', icon: 'activity', description: 'Background work and the core log', extra: true },
];

const KEY = 'pwr:inspector-tab';

/**
 * The inspector: one panel on the right, its tools as tabs -- Changes,
 * Terminal, Preview, Files, Checks -- one at a time and full height.
 * Knowledge and Activity open from the command palette. The tab is
 * remembered across launches; whether the panel shows is LayoutService's.
 */
@Injectable({ providedIn: 'root' })
export class WorkbenchStore {
  private readonly layout = inject(LayoutService);
  private readonly agent = inject(AgentStore);

  /** The tab chosen, whether or not this conversation can show it. */
  readonly tab = signal<CardId>(load());
  /** The file the Files tab should show, when another tab asks for one. */
  readonly fileRequest = signal<string | null>(null);

  /**
   * The tools this conversation can use: in chat mode, only those that need
   * no workspace.
   */
  readonly available = computed(() => (this.agent.chatMode() ? CARDS.filter((card) => !card.workspace) : CARDS));

  /** The tab showing: the one chosen, or the first this conversation can use. */
  readonly active = computed<CardId>(() => {
    const available = this.available();
    const chosen = this.tab();
    return available.some((card) => card.id === chosen) ? chosen : available[0].id;
  });

  /** The tabs in the header: the usual ones, and an extra one while it is showing. */
  readonly tabs = computed(() => this.available().filter((card) => !card.extra || card.id === this.active()));

  readonly panelVisible = computed(() => this.layout.right() !== 'hidden');

  constructor() {
    effect(() => {
      const tab = this.tab();
      try {
        localStorage.setItem(KEY, tab);
      } catch {
        // Storage unavailable: the tab is kept until the app closes.
      }
    });
  }

  isOpen(id: CardId): boolean {
    return this.panelVisible() && this.active() === id;
  }

  private usable(id: CardId): boolean {
    return this.available().some((card) => card.id === id);
  }

  /** Shows a tool's tab, opening the panel if it is closed. */
  show(id: CardId): void {
    if (!this.usable(id)) return;
    this.tab.set(id);
    if (!this.panelVisible()) this.layout.toggleRight();
  }

  /** A shortcut's toggle: shows a tool, or closes the panel when it is showing. */
  toggle(id: CardId): void {
    if (!this.usable(id)) return;
    if (this.isOpen(id)) this.layout.toggleRight();
    else this.show(id);
  }

  /** Closes an extra tab: back to the usual first one. */
  close(id: CardId): void {
    if (this.active() === id) this.tab.set(this.available()[0].id);
  }

  /** Opens the Files tab on one file. */
  openFile(path: string): void {
    this.fileRequest.set(path);
    this.show('files');
  }
}

function load(): CardId {
  try {
    const saved = localStorage.getItem(KEY);
    if (CARDS.some((card) => card.id === saved)) return saved as CardId;
  } catch {
    // Unreadable: the first tab.
  }
  return 'review';
}
