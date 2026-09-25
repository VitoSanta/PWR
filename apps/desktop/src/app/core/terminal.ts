import { Injectable, OnDestroy, effect, inject, signal } from '@angular/core';
import { ThemeService } from './theme';
import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal } from '@xterm/xterm';
import { AgentStore } from './agent.store';
import { bridge, inTauri } from './bridge';

/**
 * The person's shell in the workspace, kept alive while its card is collapsed
 * or moved: the xterm instance lives here, and the card only lends it a place
 * on the page. Closing the card ends the shell.
 */
@Injectable({ providedIn: 'root' })
export class TerminalService implements OnDestroy {
  private readonly agent = inject(AgentStore);
  private term?: Terminal;
  private fit?: FitAddon;
  private id: number | null = null;
  private workspace = '';
  private unlisten: Array<() => void> = [];
  /** The element xterm draws into, moved between hosts. */
  private readonly element = document.createElement('div');
  readonly state = signal<'idle' | 'starting' | 'running' | 'exited' | 'unavailable'>('idle');
  readonly error = signal('');

  constructor() {
    // The terminal's colours follow the app's theme, read once it is applied.
    const theme = inject(ThemeService);
    effect(() => {
      theme.theme();
      requestAnimationFrame(() => {
        if (this.term) this.term.options.theme = palette();
      });
    });
  }

  /** Shows the terminal inside `host`, starting the shell the first time. */
  async attach(host: HTMLElement): Promise<void> {
    this.element.className = 'terminal-surface';
    host.appendChild(this.element);
    if (!inTauri()) {
      this.state.set('unavailable');
      return;
    }
    const workspace = this.agent.workspace();
    if (this.term && this.workspace !== workspace) this.close();
    if (!this.term) await this.start(workspace);
    this.resize();
  }

  detach(): void {
    this.element.remove();
  }

  /** Fits the terminal to its host and tells the shell its new size. */
  resize(): void {
    if (!this.term || !this.fit || !this.element.isConnected) return;
    try {
      this.fit.fit();
    } catch {
      return;
    }
    if (this.id !== null) void bridge.termResize(this.id, this.term.cols, this.term.rows);
  }

  focus(): void {
    this.term?.focus();
  }

  /** Ends the shell; the next attach starts a new one. */
  close(): void {
    if (this.id !== null) void bridge.termClose(this.id);
    for (const stop of this.unlisten) stop();
    this.unlisten = [];
    this.term?.dispose();
    this.term = undefined;
    this.fit = undefined;
    this.id = null;
    this.element.replaceChildren();
    this.state.set('idle');
  }

  /** A fresh shell after the last one exited. */
  async restart(): Promise<void> {
    const host = this.element.parentElement;
    this.close();
    if (host) await this.attach(host);
  }

  ngOnDestroy(): void {
    this.close();
  }

  private async start(workspace: string): Promise<void> {
    this.state.set('starting');
    this.error.set('');
    this.workspace = workspace;
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
    term.open(this.element);
    this.term = term;
    this.fit = fit;
    try {
      fit.fit();
    } catch {
      // Not laid out yet: fitted on the next resize.
    }
    try {
      const id = await bridge.termOpen(workspace, term.cols || 80, term.rows || 24);
      this.id = id;
      term.onData((data) => void bridge.termWrite(id, data));
      this.unlisten.push(
        await bridge.onTermOutput((output) => {
          if (output.id === id) term.write(output.data);
        }),
        await bridge.onTermExit((exited) => {
          if (exited !== id) return;
          this.id = null;
          this.state.set('exited');
          term.write('\r\n\x1b[2m[shell ended]\x1b[0m\r\n');
        }),
      );
      this.state.set('running');
      term.focus();
    } catch (error) {
      this.state.set('exited');
      this.error.set(String(error).replace(/^Error: /, ''));
    }
  }
}

function token(name: string, fallback: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/** xterm's colours, from the app's theme tokens. */
function palette() {
  return {
    background: token('--surface-primary', '#101621'),
    foreground: token('--text-primary', '#e6ebf5'),
    cursor: token('--accent-solid', '#6e5ff0'),
    cursorAccent: token('--surface-primary', '#101621'),
    selectionBackground: token('--accent-border', 'rgba(110, 95, 240, 0.4)'),
  };
}
