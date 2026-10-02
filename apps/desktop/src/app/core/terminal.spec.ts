import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { AgentStore } from './agent.store';
import { ThemeService } from './theme';
import { bridge } from './bridge';
import { TerminalService } from './terminal';

vi.mock('@xterm/xterm', () => ({ Terminal: class {
  cols = 80; rows = 24; options = {}; buffer = { active: { length: 0 } };
  loadAddon() {} open() {} onData() {} dispose() {} focus() {} write() {}
} }));
vi.mock('@xterm/addon-fit', () => ({ FitAddon: class { fit() {} } }));

describe('Terminal output for the core', () => {
  it('closes a shell whose open operation finishes after the terminal was closed', async () => {
    terminal.close();
    let resolve!: (id: number) => void;
    const open = vi.spyOn(bridge, 'termOpen').mockImplementation(() => new Promise(done => { resolve = done; }));
    const close = vi.spyOn(bridge, 'termClose').mockResolvedValue(undefined as any);
    vi.spyOn(bridge, 'onTermOutput').mockResolvedValue(() => {});
    vi.spyOn(bridge, 'onTermExit').mockResolvedValue(() => {});
    const starting = (terminal as any).start('/project-a');
    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    terminal.close();
    resolve(71);
    await starting;
    expect(close).toHaveBeenCalledWith(71);
    expect(terminal.state()).toBe('idle');
  });

  let terminal: TerminalService;
  let workspace: ReturnType<typeof signal<string>>;
  beforeEach(() => {
    workspace = signal('/project-a');
    TestBed.configureTestingModule({ providers: [
      { provide: AgentStore, useValue: { workspace, terminalReader: null } },
      { provide: ThemeService, useValue: { theme: () => 'light', palette: () => 'layout' } },
    ] });
    terminal = TestBed.inject(TerminalService);
    const rows = [
      { text: 'Module not found: ', wrapped: false },
      { text: './styles.css', wrapped: true },
      { text: 'Build failed', wrapped: false },
      { text: '', wrapped: false },
    ];
    Object.assign(terminal as any, { workspace: '/project-a', term: {
      buffer: { active: { length: rows.length, getLine: (row: number) => ({
        isWrapped: rows[row].wrapped, translateToString: () => rows[row].text,
      }) } }, dispose: vi.fn(),
    } });
    terminal.state.set('running');
  });

  it('returns plain text with wrapped rows joined and trailing blank rows removed', () => {
    expect(terminal.read(100)).toEqual([{ title: 'Terminal', running: true,
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
