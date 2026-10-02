import { Injectable, OnDestroy, WritableSignal, computed, effect, inject, signal } from '@angular/core';
import { ThemeService } from './theme';
import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal } from '@xterm/xterm';
import { AgentStore, TerminalSnapshot } from './agent.store';
import { bridge, inTauri } from './bridge';

export type ShellState = 'starting' | 'running' | 'exited';

/** One shell: its xterm, drawn into an element the card lends a place. */
interface Session {
  key: number;
  title: string;
  readonly state: WritableSignal<ShellState>;
  readonly error: WritableSignal<string>;
  element: HTMLDivElement;
  term?: Terminal;
  fit?: FitAddon;
  id: number | null;
  unlisten: Array<() => void>;
}

/**
 * The person's shells in the workspace, as tabs -- several at once, as in
 * VS Code -- kept alive while the card is collapsed, moved or closed: each
 * xterm lives here, and the card only lends the chosen one a place on the
 * page. Closing a tab ends its shell.
 */
@Injectable({ providedIn: 'root' })
export class TerminalService implements OnDestroy {
  private readonly agent = inject(AgentStore);
  private workspace = '';
  private host: HTMLElement | null = null;
  private counter = 0;
  /** Outside the app there is no shell to start. */
  readonly unavailable = signal(false);
  readonly sessions = signal<Session[]>([]);
  readonly active = signal<number | null>(null);
  readonly current = computed(() => this.sessions().find((session) => session.key === this.active()) ?? null);
  /** The shown shell's state and error, for the card. */
  readonly state = computed(() => this.current()?.state() ?? null);
  readonly error = computed(() => this.current()?.error() ?? '');

  constructor() {
    // What the model may read, when the person allows it: these tabs' text.
    this.agent.terminalReader = (lines) => this.read(lines);
    // The terminals' colours follow the app's theme, read once it is applied.
    const theme = inject(ThemeService);
    effect(() => {
      theme.theme();
      theme.palette();
      requestAnimationFrame(() => {
        for (const session of this.sessions()) if (session.term) session.term.options.theme = palette();
      });
    });
  }

  /** Shows the chosen shell inside `host`, starting the first one the first time. */
  async attach(host: HTMLElement): Promise<void> {
    this.host = host;
    if (!inTauri()) {
      this.unavailable.set(true);
      return;
    }
    const workspace = this.agent.workspace();
    // Another workspace: the shells of the last one end, a new one starts here.
    if (this.sessions().length && this.workspace !== workspace) this.closeAll();
    this.workspace = workspace;
    if (!this.sessions().length) {
      await this.add();
      return;
    }
    this.show();
  }

  detach(): void {
    this.current()?.element.remove();
    this.host = null;
  }

  /** A new shell, in a tab of its own, shown. */
  async add(): Promise<void> {
    if (!inTauri()) return;
    const session: Session = {
      key: ++this.counter,
      title: `Terminal ${this.counter}`,
      state: signal<ShellState>('starting'),
      error: signal(''),
      element: document.createElement('div'),
      id: null,
      unlisten: [],
    };
    session.element.className = 'terminal-surface';
    this.sessions.update((sessions) => [...sessions, session]);
    this.select(session.key);
    await this.start(session);
  }

  select(key: number): void {
    if (!this.sessions().some((session) => session.key === key)) return;
    this.current()?.element.remove();
    this.active.set(key);
    this.show();
  }

  /** Ends one shell and its tab; the one beside it is shown. */
  close(key: number): void {
    const sessions = this.sessions();
    const index = sessions.findIndex((session) => session.key === key);
    if (index < 0) return;
    const session = sessions[index];
    end(session);
    session.element.remove();
    const rest = sessions.filter((item) => item.key !== key);
    this.sessions.set(rest);
    if (this.active() === key) {
      const next = rest[Math.min(index, rest.length - 1)];
      this.active.set(next?.key ?? null);
      this.show();
    }
  }

  /** A fresh shell in the shown tab, after its last one exited. */
  async restart(): Promise<void> {
    const session = this.current();
    if (!session) return this.add();
    end(session);
    session.error.set('');
    session.state.set('starting');
    await this.start(session);
  }

  /** Fits the shown terminal to its host and tells the shell its new size. */
  resize(): void {
    const session = this.current();
    if (!session?.term || !session.fit || !session.element.isConnected) return;
    try {
      session.fit.fit();
    } catch {
      return;
    }
    if (session.id !== null) void bridge.termResize(session.id, session.term.cols, session.term.rows);
  }

  focus(): void {
    this.current()?.term?.focus();
  }

  /**
   * The last `lines` rows of each tab, as plain text -- xterm's buffer, so no
   * colour codes -- with a line the terminal wrapped joined back to one.
   */
  read(lines: number): TerminalSnapshot[] {
    return this.sessions()
      .filter((session) => session.term)
      .map((session) => {
        const buffer = session.term!.buffer.active;
        const rows: string[] = [];
        for (let row = Math.max(0, buffer.length - lines); row < buffer.length; row++) {
          const line = buffer.getLine(row);
          const text = line?.translateToString(true) ?? '';
          if (line?.isWrapped && rows.length) rows[rows.length - 1] += text;
          else rows.push(text);
        }
        while (rows.length && !rows[rows.length - 1].trim()) rows.pop();
        return { title: session.title, running: session.state() === 'running', text: rows.join('\n') };
      });
  }

  ngOnDestroy(): void {
    if (this.agent.terminalReader) this.agent.terminalReader = null;
    this.closeAll();
  }

  private closeAll(): void {
    for (const session of this.sessions()) {
      end(session);
      session.element.remove();
    }
    this.sessions.set([]);
    this.active.set(null);
  }

  /** Puts the chosen shell in the card, sized to it. */
  private show(): void {
    const session = this.current();
    if (!session || !this.host) return;
    if (session.element.parentElement !== this.host) this.host.appendChild(session.element);
    requestAnimationFrame(() => {
      this.resize();
      this.focus();
    });
  }

  private async start(session: Session): Promise<void> {
    const [{ Terminal }, { FitAddon }] = await Promise.all([import('@xterm/xterm'), import('@xterm/addon-fit')]);
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: token('--mono', 'ui-monospace, Menlo, monospace'),
      fontSize: 12.5,
      lineHeight: 1.25,
      scrollback: 5000,
      theme: palette(),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(session.element);
    session.term = term;
    session.fit = fit;
    try {
      fit.fit();
    } catch {
      // Not laid out yet: fitted on the next resize.
    }
    try {
      const id = await bridge.termOpen(this.workspace, term.cols || 80, term.rows || 24);
      // Closed while it was starting.
      if (!this.sessions().includes(session)) {
        void bridge.termClose(id);
        return;
      }
      session.id = id;
      term.onData((data) => void bridge.termWrite(id, data));
      session.unlisten.push(
        await bridge.onTermOutput((output) => {
          if (output.id === id) term.write(output.data);
        }),
        await bridge.onTermExit((exited) => {
          if (exited !== id) return;
          session.id = null;
          session.state.set('exited');
          term.write('\r\n\x1b[2m[shell ended]\x1b[0m\r\n');
        }),
      );
      session.state.set('running');
      if (this.active() === session.key) {
        this.resize();
        term.focus();
      }
    } catch (error) {
      session.state.set('exited');
      session.error.set(String(error).replace(/^Error: /, ''));
    }
  }
}

/** Ends a session's shell and lets its xterm go. */
function end(session: Session): void {
  if (session.id !== null) void bridge.termClose(session.id);
  for (const stop of session.unlisten) stop();
  session.unlisten = [];
  session.term?.dispose();
  session.term = undefined;
  session.fit = undefined;
  session.id = null;
  session.element.replaceChildren();
}

function token(name: string, fallback: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * xterm's colours, from the app's theme tokens -- the ANSI ones too, so a
 * program's red, green or white stays readable on a light palette as on a
 * dark one. xterm's own defaults stand in where a token is missing.
 */
function palette() {
  const ansi = (name: string) => token(name, '') || undefined;
  return {
    background: token('--surface-primary', '#101621'),
    foreground: token('--text-primary', '#e6ebf5'),
    cursor: token('--accent-solid', '#6e5ff0'),
    cursorAccent: token('--surface-primary', '#101621'),
    selectionBackground: token('--accent-border', 'rgba(110, 95, 240, 0.4)'),
    black: ansi('--text-muted'),
    red: ansi('--danger-text'),
    green: ansi('--success-text'),
    yellow: ansi('--warning-text'),
    blue: ansi('--accent-text'),
    magenta: ansi('--data-7'),
    cyan: ansi('--accent-secondary-text'),
    white: ansi('--text-secondary'),
    brightBlack: ansi('--text-muted'),
    brightRed: ansi('--danger'),
    brightGreen: ansi('--success'),
    brightYellow: ansi('--warning'),
    brightBlue: ansi('--accent-primary'),
    brightMagenta: ansi('--data-7'),
    brightCyan: ansi('--accent-secondary'),
    brightWhite: ansi('--text-primary'),
  };
}
