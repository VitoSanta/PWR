import { Injectable, signal } from '@angular/core';

export const isMac =
  typeof navigator !== 'undefined' && /Macintosh|Mac OS X/.test(navigator.userAgent);

/** "⌘K" on macOS, "Ctrl+K" elsewhere. */
export function shortcut(keys: string): string {
  if (isMac)
    return keys
      .replace(/Mod\+/g, '⌘')
      .replace(/Shift\+/g, '⇧')
      .replace(/Alt\+/g, '⌥');
  return keys.replace(/Mod\+/g, 'Ctrl+');
}

/** The application's keyboard shortcuts, in one place so labels and handlers agree. */
export const SHORTCUTS = {
  palette: 'Mod+K',
  newConversation: 'Mod+N',
  toggleSidebar: 'Mod+B',
  toggleInspector: 'Mod+Alt+B',
  settings: 'Mod+,',
} as const;

/** Application-level surfaces that are not owned by one feature. */
@Injectable({ providedIn: 'root' })
export class UiStore {
  readonly settingsOpen = signal(false);
  readonly paletteOpen = signal(false);
  /** Kept here so the tab survives the inspector docking and undocking. */
  readonly inspectorTab = signal<'changes' | 'evidence' | 'wiki' | 'log'>('changes');
}

/**
 * The open modal dialogs, innermost last. Escape closes only the innermost,
 * and only after anything inside it (a select, a menu) has had the key.
 */
@Injectable({ providedIn: 'root' })
export class DialogStack {
  private readonly stack: { close(): void }[] = [];

  constructor() {
    if (typeof window === 'undefined') return;
    window.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape' || event.defaultPrevented || !this.stack.length) return;
      event.preventDefault();
      this.stack[this.stack.length - 1].close();
    });
  }

  get open(): boolean {
    return this.stack.length > 0;
  }

  push(dialog: { close(): void }): void {
    this.stack.push(dialog);
  }

  remove(dialog: { close(): void }): void {
    const index = this.stack.indexOf(dialog);
    if (index >= 0) this.stack.splice(index, 1);
  }
}

export interface Toast {
  id: number;
  text: string;
  tone: 'neutral' | 'success' | 'danger';
}

@Injectable({ providedIn: 'root' })
export class ToastService {
  readonly toasts = signal<Toast[]>([]);
  private next = 1;

  show(text: string, tone: Toast['tone'] = 'neutral', ms = 3200): void {
    const id = this.next++;
    this.toasts.update((list) => [...list.slice(-2), { id, text, tone }]);
    setTimeout(() => this.dismiss(id), ms);
  }

  dismiss(id: number): void {
    this.toasts.update((list) => list.filter((toast) => toast.id !== id));
  }
}

export interface ConfirmRequest {
  title: string;
  message?: string;
  /** The thing acted on: a file, a folder, a conversation. */
  subject?: string;
  /** Paths and commands are set in the monospace face; names are not. */
  subjectIsText?: boolean;
  confirmLabel: string;
  tone?: 'danger' | 'primary';
  /** Run while the dialog stays open; a failure is shown in it. */
  action?: () => Promise<void>;
}

/** One confirmation dialog for the whole app, instead of `window.confirm`. */
@Injectable({ providedIn: 'root' })
export class ConfirmService {
  readonly request = signal<(ConfirmRequest & { resolve: (ok: boolean) => void }) | null>(null);

  ask(request: ConfirmRequest): Promise<boolean> {
    this.request()?.resolve(false);
    return new Promise((resolve) => this.request.set({ ...request, resolve }));
  }

  settle(ok: boolean): void {
    const current = this.request();
    this.request.set(null);
    current?.resolve(ok);
  }
}

/** Moves focus among a container's items with the arrow keys, Home and End. */
export function roveFocus(
  event: KeyboardEvent,
  container: HTMLElement,
  selector: string,
  orientation: 'vertical' | 'horizontal' = 'vertical',
): boolean {
  const next = orientation === 'vertical' ? 'ArrowDown' : 'ArrowRight';
  const previous = orientation === 'vertical' ? 'ArrowUp' : 'ArrowLeft';
  if (![next, previous, 'Home', 'End'].includes(event.key)) return false;
  const items = Array.from(container.querySelectorAll<HTMLElement>(selector)).filter(
    (item) => !item.hasAttribute('disabled') && item.getAttribute('aria-disabled') !== 'true',
  );
  if (!items.length) return false;
  const index = items.indexOf(document.activeElement as HTMLElement);
  let target = index;
  if (event.key === 'Home') target = 0;
  else if (event.key === 'End') target = items.length - 1;
  else if (event.key === next) target = index < 0 ? 0 : (index + 1) % items.length;
  else target = index < 0 ? items.length - 1 : (index - 1 + items.length) % items.length;
  items[target].focus();
  event.preventDefault();
  return true;
}
