import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { AgentStore } from './agent.store';
import { ThemeService } from './theme';
import { bridge } from './bridge';
import { TerminalService, TerminalSession } from './terminal';

vi.mock('@xterm/xterm', () => ({ Terminal: class {
  cols = 80; rows = 24; options = {}; buffer = { active: { length: 0 } };
  loadAddon() {} open() {} onData() {} dispose() {} focus() {} write() {}
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

  let terminal: TerminalService;
  let session: TerminalSession;
  let workspace: ReturnType<typeof signal<string>>;
  beforeEach(() => {
    workspace = signal('/project-a');
    TestBed.configureTestingModule({ providers: [
      { provide: AgentStore, useValue: { workspace, terminalReader: null } },
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
      {provide: AgentStore, useValue: {workspace: signal('/work'), terminalReader: null}},
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
