import { Injectable, OnDestroy, computed, effect, inject, signal, untracked } from '@angular/core';
import { WorkbenchStore } from './workbench';
import { ThemeService } from './theme';
import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal } from '@xterm/xterm';
import { AgentStore, TerminalSnapshot } from './agent.store';
import { bridge, inTauri } from './bridge';

/** Multiple independent shells; moving or collapsing the card preserves them. */
@Injectable({ providedIn: 'root' })
export class TerminalService implements OnDestroy {
  private readonly agent = inject(AgentStore);
  private readonly work = inject(WorkbenchStore);
  readonly sessions = signal<TerminalSession[]>([]);
  readonly activeId = signal<number | null>(null);
  readonly active = computed(() => this.sessions().find(s => s.key === this.activeId()));
  readonly state = computed(() => this.active()?.state() ?? 'idle');
  readonly error = computed(() => this.active()?.error() ?? '');
  private host?: HTMLElement;
  private sequence = 0;

  constructor() {
    this.agent.terminalReader = lines => this.read(lines);
    this.agent.workspaceStops.add(() => this.sessions().filter(session => session.modelOwned).forEach(session => { session.takeControl(); void session.interrupt(); }));
    this.agent.workspaceHandlers.set('terminal', async (operation, args) => {
      this.work.show('terminal');
      if (operation === 'start') {
        if (this.sessions().length >= 16) throw new Error('Close a terminal before opening another.');
        if (typeof args.executable !== 'string' || !Array.isArray(args.args ?? []) || !(args.args ?? []).every((arg: unknown) => typeof arg === 'string')) throw new Error('Specify executable and args.');
        if (args.cwd !== this.agent.workspace()) throw new Error('The workspace changed.');
        const epoch = this.agent.workspaceEpoch();
        const session = new TerminalSession(++this.sequence, this.agent.workspace(), args, this.agent.sessionId());
        this.active()?.detach(); this.sessions.update(all => [...all, session]); this.activeId.set(session.key);
        await session.attach(this.host ?? document.createElement('div'));
        if (epoch !== this.agent.workspaceEpoch()) { session.close(); throw new Error('The turn was cancelled.'); }
        if (session.state() === 'exited' && session.error()) throw new Error(session.error());
        return { id: session.key, state: session.state(), sandboxed: args.sandboxed, output: session.read(100) };
      }
      const session = this.sessions().find(item => item.key === Number(args.id));
      if (!session || !session.modelOwned || session.conversationId !== this.agent.sessionId()) throw new Error('Choose a process started by this conversation.');
      if (operation === 'read') {
        const wait = Math.max(0, Math.min(2000, Number(args.waitMs) || 0));
        if (wait) await new Promise(resolve => setTimeout(resolve, wait));
        return { id: session.key, state: session.state(), controller: session.controller(), output: session.read(Number(args.lines) || 200) };
      }
      if (session.controller() !== 'model') throw new Error('The engineer has taken control. Wait until they return control to PWR.');
      if (operation === 'input') await session.send(String(args.data ?? '').slice(0, 16384));
      else if (operation === 'interrupt') await session.interrupt();
      else if (operation === 'close') await this.closeSession(session.key);
      else throw new Error('Unknown terminal operation.');
      return { id: session.key, state: session.state(), output: session.read(100) };
    });
    effect(() => {
      const sessionId = this.agent.sessionId();
      untracked(() => this.sessions().filter(item => item.modelOwned && item.conversationId !== sessionId).forEach(item => void this.closeSession(item.key)));
    });
    const theme = inject(ThemeService);
    effect(() => {
      theme.theme(); theme.palette();
      requestAnimationFrame(() => this.sessions().forEach(session => session.updateTheme()));
    });
    effect(() => {
      const workspace = this.agent.workspace();
      if (this.sessions().some(s => s.cwd !== workspace)) untracked(() => this.close());
    });
  }

  async attach(host: HTMLElement): Promise<void> {
    this.host = host;
    if (!this.active()) await this.newSession();
    else await this.active()!.attach(host);
  }
  detach(): void { this.active()?.detach(); this.host = undefined; }
  resize(): void { this.active()?.resize(); }
  focus(): void { this.active()?.focus(); }
  async newSession(): Promise<void> {
    this.active()?.detach();
    const session = new TerminalSession(++this.sequence, this.agent.workspace());
    this.sessions.update(s => [...s, session]);
    this.activeId.set(session.key);
    if (this.host) {
      await session.attach(this.host);
      if (session === this.active()) session.focus();
    }
  }
  async select(key: number): Promise<void> {
    const session = this.sessions().find(s => s.key === key);
    if (!session || session === this.active()) return;
    this.active()?.detach();
    this.activeId.set(key);
    if (this.host) {
      await session.attach(this.host);
      if (session === this.active()) session.focus();
    }
  }
  async closeSession(key: number): Promise<void> {
    const index = this.sessions().findIndex(s => s.key === key);
    if (index < 0) return;
    const wasActive = this.activeId() === key;
    this.sessions()[index].close();
    this.sessions.update(s => s.filter(item => item.key !== key));
    if (wasActive) {
      const next = this.sessions()[Math.min(index, this.sessions().length - 1)];
      this.activeId.set(next?.key ?? null);
      if (next && this.host) await next.attach(this.host);
    }
  }
  close(): void {
    this.sessions().forEach(s => s.close());
    this.sessions.set([]); this.activeId.set(null);
  }
  async restart(): Promise<void> { await this.active()?.restart(); }
  read(lines: number): TerminalSnapshot[] {
    return this.sessions().filter(s => s.cwd === this.agent.workspace()).flatMap(s => s.read(lines));
  }
  ngOnDestroy(): void { this.agent.terminalReader = null; this.close(); }
}

/** A PTY and its xterm buffer belong to one tab, including while inactive. */
export class TerminalSession {
  private term?: Terminal;
  private fit?: FitAddon;
  private id: number | null = null;
  private startup = 0;
  private unlisten: Array<() => void> = [];
  private readonly element = document.createElement('div');
  readonly state = signal<'idle' | 'starting' | 'running' | 'exited' | 'unavailable'>('idle');
  readonly error = signal('');
  readonly controller = signal<'user' | 'model'>('user');
  readonly modelOwned: boolean;
  private starting?: Promise<void>;
  constructor(readonly key: number, readonly cwd: string, readonly launch?: any, readonly conversationId: string | null = null) {
    this.modelOwned = !!launch; this.controller.set(launch ? 'model' : 'user');
  }
  async send(data: string): Promise<void> {
    if (this.id === null || this.state() !== 'running') throw new Error('This process has ended.');
    await bridge.termWrite(this.id, data);
  }
  async interrupt(): Promise<void> { if (this.id !== null) await bridge.termWrite(this.id, '\x03'); }
  takeControl(): void { this.controller.set('user'); this.focus(); }
  giveControl(): void { if (this.modelOwned) this.controller.set('model'); }
  updateTheme(): void { if (this.term) this.term.options.theme = palette(); }

  /** Shows the terminal inside `host`, starting the shell the first time. */
  async attach(host: HTMLElement): Promise<void> {
    this.element.className = 'terminal-surface';
    host.appendChild(this.element);
    if (!inTauri()) {
      this.state.set('unavailable');
      return;
    }
    if (!this.term) {
      this.starting ??= this.start(this.cwd).finally(() => { this.starting = undefined; });
      await this.starting;
    }
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
    this.startup++;
    if (this.id !== null) void bridge.termClose(this.id);
    for (const stop of this.unlisten) stop();
    this.unlisten = [];
    this.term?.dispose();
    this.term = undefined;
    this.fit = undefined;
    this.id = null;
    this.element.remove();
    this.element.replaceChildren();
    this.state.set('idle');
  }

  /** A fresh shell after the last one exited. */
  async restart(): Promise<void> {
    const host = this.element.parentElement;
    this.close();
    if (host) await this.attach(host);
  }

  /** Current workspace terminal buffer as text, preserving wrapped rows. */
  read(lines: number): TerminalSnapshot[] {
    if (!this.term) return [];
    const count = Math.floor(Math.max(1, Math.min(1000, Number(lines) || 200)));
    const buffer = this.term.buffer.active;
    const rows: string[] = [];
    for (let row = Math.max(0, buffer.length - count); row < buffer.length; row++) {
      const line = buffer.getLine(row);
      const text = line?.translateToString(true) ?? '';
      if (line?.isWrapped && rows.length) rows[rows.length - 1] += text;
      else rows.push(text);
    }
    while (rows.length && !rows[rows.length - 1].trim()) rows.pop();
    return [{ title: `Terminal ${this.key}`, running: this.state() === 'running', text: rows.join('\n').slice(-64000) }];
  }

  private async start(workspace: string): Promise<void> {
    const startup = ++this.startup;
    this.state.set('starting');
    this.error.set('');
    const [{ Terminal }, { FitAddon }] = await Promise.all([import('@xterm/xterm'), import('@xterm/addon-fit')]);
    if (startup !== this.startup) return;
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
    // What was started, as a shell would show it: the engineer sees the command, and so does read_terminal.
    if (this.launch) {
      const quote = (word: string) => /^[\w@%+=:,./-]+$/.test(word) ? word : `'${word.replace(/'/g, `'\\''`)}'`;
      term.write(`\x1b[2m$ ${[this.launch.executable, ...(this.launch.args ?? [])].map(String).map(quote).join(' ')}\x1b[0m\r\n`);
    }
    try {
      fit.fit();
    } catch {
      // Not laid out yet: fitted on the next resize.
    }
    const listeners: Array<() => void> = [];
    let id: number | null = null;
    let opening = true;
    const early: Array<{ id: number; data?: string }> = [];
    let earlyBytes = 0;
    const receive = (event: { id: number; data?: string }) => {
      if (startup !== this.startup) return;
      if (opening) {
        // term_open can emit the prompt (or exit) before its IPC reply.
        if (earlyBytes < 65536 && early.length < 1024) {
          const data = event.data?.slice(0, 65536 - earlyBytes);
          early.push({ id: event.id, data });
          earlyBytes += data?.length ?? 0;
        }
        return;
      }
      if (event.id !== id) return;
      if (event.data !== undefined) term.write(event.data);
      else {
        this.id = null;
        this.state.set('exited');
        term.write('\r\n\x1b[2m[shell ended]\x1b[0m\r\n');
      }
    };
    try {
      listeners.push(await bridge.onTermOutput(receive));
      listeners.push(await bridge.onTermExit(exited => receive({ id: exited })));
      if (startup !== this.startup) {
        listeners.forEach(stop => stop());
        return;
      }
      id = await bridge.termOpen(workspace, term.cols || 80, term.rows || 24, this.launch);
      if (startup !== this.startup) {
        listeners.forEach(stop => stop());
        await bridge.termClose(id);
        return;
      }
      this.id = id;
      this.unlisten.push(...listeners);
      this.state.set('running');
      opening = false;
      early.forEach(receive);
      early.length = 0;
      const shellId = id;
      term.onData((data) => {
        if (this.id === shellId && this.controller() === 'user') void bridge.termWrite(shellId, data).catch(error => this.error.set(String(error)));
      });
    } catch (error) {
      listeners.forEach(stop => stop());
      if (id !== null) await bridge.termClose(id).catch(() => {});
      if (startup !== this.startup) return;
      this.id = null;
      this.state.set('exited');
      this.error.set(String(error).replace(/^Error: /, ''));
    }
  }
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
  // Resolve light-dark()/color-mix() in the document's colour scheme before
  // passing colours to xterm's canvas, whose own scheme can differ.
  const probe = document.createElement('span');
  probe.style.display = 'none';
  document.documentElement.appendChild(probe);
  const colour = (name: string, fallback: string) => {
    probe.style.color = '';
    probe.style.color = `var(${name}, ${fallback})`;
    return getComputedStyle(probe).color || fallback;
  };
  const ansi = (name: string) => colour(name, '#808080');
  const colours = {
    background: colour('--surface-primary', '#101621'),
    foreground: colour('--text-primary', '#e6ebf5'),
    cursor: colour('--accent-solid', '#6e5ff0'),
    cursorAccent: colour('--surface-primary', '#101621'),
    selectionBackground: colour('--accent-border', 'rgba(110, 95, 240, 0.4)'),
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
  probe.remove();
  return colours;
}
