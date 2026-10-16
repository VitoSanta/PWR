import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { AgentStore } from './agent.store';
import { ThemeService } from './theme';
import { WorkbenchStore } from './workbench';
import { bridge } from './bridge';
import { TerminalService, TerminalSession } from './terminal';

vi.mock('@xterm/xterm', () => ({ Terminal: class {
  cols = 80; rows = 24; options = {}; buffer = { active: { length: 0 } };
  loadAddon() {} open() {} onData() {} dispose() {} focus() {} write = vi.fn();
} }));
vi.mock('@xterm/addon-fit', () => ({ FitAddon: class { fit() {} } }));

describe('Terminal output for the core', () => {
  it('closes a shell whose open operation finishes after the terminal was closed', async () => {
    session.close();
    let resolve!: (id: number) => void;
    const open = vi.spyOn(bridge, 'termOpen').mockImplementation(() => new Promise(done => { resolve = done; }));
    const close = vi.spyOn(bridge, 'termClose').mockResolvedValue(undefined as any);
    vi.spyOn(bridge, 'onTermOutput').mockResolvedValue(() => {});
    vi.spyOn(bridge, 'onTermExit').mockResolvedValue(() => {});
    const starting = (session as any).start('/project-a');
    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    session.close();
    resolve(71);
    await starting;
    expect(close).toHaveBeenCalledWith(71);
    expect(session.state()).toBe('idle');
  });

  it('preserves output and an immediate exit emitted before term_open replies', async () => {
    session.close();
    let output!: (event: { id: number; data: string }) => void;
    let exited!: (id: number) => void;
    const stopOutput = vi.fn(), stopExit = vi.fn();
    vi.spyOn(bridge, 'onTermOutput').mockImplementation(async handler => { output = handler; return stopOutput; });
    vi.spyOn(bridge, 'onTermExit').mockImplementation(async handler => { exited = handler; return stopExit; });
    vi.spyOn(bridge, 'termOpen').mockImplementation(async () => {
      output({ id: 72, data: 'another shell' });
      output({ id: 71, data: 'prompt> ' });
      exited(71);
      return 71;
    });
    await (session as any).start('/project-a');
    const write = (session as any).term.write;
    expect(write).toHaveBeenCalledWith('prompt> ');
    expect(write).not.toHaveBeenCalledWith('another shell');
    expect(session.state()).toBe('exited');
    session.close();
    expect(stopOutput).toHaveBeenCalledOnce();
    expect(stopExit).toHaveBeenCalledOnce();
  });

  let terminal: TerminalService;
  let session: TerminalSession;
  let workspace: ReturnType<typeof signal<string>>;
  beforeEach(() => {
    workspace = signal('/project-a');
    TestBed.configureTestingModule({ providers: [
      { provide: AgentStore, useValue: { workspace, terminalReader: null, sessionId: () => null, workspaceHandlers: new Map(), workspaceStops: new Set(), workspaceEpoch: () => 0 } },
      { provide: ThemeService, useValue: { theme: () => 'light', palette: () => 'layout' } },
    ] });
    terminal = TestBed.inject(TerminalService);
    session = new TerminalSession(1, '/project-a');
    terminal.sessions.set([session]); terminal.activeId.set(1);
    const rows = [
      { text: 'Module not found: ', wrapped: false },
      { text: './styles.css', wrapped: true },
      { text: 'Build failed', wrapped: false },
      { text: '', wrapped: false },
    ];
    Object.assign(session as any, { term: {
      buffer: { active: { length: rows.length, getLine: (row: number) => ({
        isWrapped: rows[row].wrapped, translateToString: () => rows[row].text,
      }) } }, dispose: vi.fn(),
    } });
    session.state.set('running');
  });

  it('returns plain text with wrapped rows joined and trailing blank rows removed', () => {
    expect(terminal.read(100)).toEqual([{ title: 'Terminal 1', running: true,
      text: 'Module not found: ./styles.css\nBuild failed' }]);
  });

  it('respects the requested tail and removes the reader when the shell closes', () => {
    expect(terminal.read(2)[0].text).toBe('Build failed');
    terminal.close();
    expect(terminal.read(2)).toEqual([]);
  });

  it('does not expose a terminal from another workspace', () => {
    workspace.set('/project-b');
    expect(terminal.read(100)).toEqual([]);
  });
});

describe('Independent terminal sessions', () => {
  it('keeps inactive buffers, selects another shell, and closes only the chosen tab', async () => {
    TestBed.configureTestingModule({providers: [
      {provide: AgentStore, useValue: {workspace: signal('/work'), terminalReader: null, sessionId: () => null, workspaceHandlers: new Map(), workspaceStops: new Set(), workspaceEpoch: () => 0}},
      {provide: ThemeService, useValue: {theme: () => 'light', palette: () => 'layout'}},
    ]});
    const terminal = TestBed.inject(TerminalService);
    await terminal.newSession(); const first = terminal.active()!;
    await terminal.newSession(); const second = terminal.active()!;
    const closeFirst = vi.spyOn(first, 'close');
    const closeSecond = vi.spyOn(second, 'close');
    await terminal.select(first.key);
    expect(terminal.active()).toBe(first);
    expect(terminal.sessions()).toEqual([first, second]);
    expect(closeSecond).not.toHaveBeenCalled();
    await terminal.closeSession(first.key);
    expect(closeFirst).toHaveBeenCalledOnce();
    expect(closeSecond).not.toHaveBeenCalled();
    expect(terminal.active()).toBe(second);
    terminal.close();
  });
});

describe('Shared terminal ownership', () => {
  it('rejects private shells and pauses model input after human takeover', async () => {
    const agent = { workspace: signal('/work'), sessionId: signal('conversation-a'), workspaceEpoch: signal(0),
      terminalReader: null, workspaceHandlers: new Map(), workspaceStops: new Set<() => void>() };
    TestBed.configureTestingModule({providers: [
      {provide: AgentStore, useValue: agent},
      {provide: WorkbenchStore, useValue: {show: vi.fn()}},
      {provide: ThemeService, useValue: {theme: () => 'light', palette: () => 'layout'}},
    ]});
    const terminal = TestBed.inject(TerminalService);
    await terminal.newSession();
    const interact = agent.workspaceHandlers.get('terminal');
    await expect(interact('input', {id: terminal.active()!.key, data: 'private'})).rejects.toThrow(/started by this conversation/);
    vi.spyOn(TerminalSession.prototype, 'attach').mockImplementation(async function (this: TerminalSession) { this.state.set('running'); });
    const started = await interact('start', {executable: 'python3', args: [], cwd: '/work', sandboxed: true});
    const shared = terminal.active()!;
    expect(shared.modelOwned).toBe(true);
    expect(shared.conversationId).toBe('conversation-a');
    const send = vi.spyOn(shared, 'send').mockResolvedValue();
    const interrupt = vi.spyOn(shared, 'interrupt').mockResolvedValue();
    await interact('input', {id: started.id, data: 'hello'});
    expect(send).toHaveBeenCalledWith('hello');
    agent.workspaceStops.forEach(stop => stop());
    expect(interrupt).toHaveBeenCalledOnce();
    await expect(interact('input', {id: started.id, data: 'blocked'})).rejects.toThrow(/taken control/);
    shared.giveControl();
    agent.sessionId.set('conversation-b');
    await expect(interact('read', {id: started.id})).rejects.toThrow(/started by this conversation/);
    terminal.close();
  });
});
