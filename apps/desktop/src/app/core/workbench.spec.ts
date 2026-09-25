import { TestBed } from '@angular/core/testing';
import { normalize } from '../ui/workbench/cards';
import { AgentStore } from './agent.store';
import { WorkbenchStore } from './workbench';

describe('WorkbenchStore', () => {
  beforeEach(() => {
    localStorage.removeItem('pwr:workbench');
    TestBed.configureTestingModule({});
  });

  it('opens, collapses, maximises, moves and closes cards, and remembers them', () => {
    const work = TestBed.inject(WorkbenchStore);
    work.show('terminal');
    work.show('knowledge');
    expect(work.open().map((card) => card.id)).toEqual(['review', 'terminal', 'knowledge']);

    work.collapse('terminal');
    expect(work.open()[1].collapsed).toBe(true);
    // Showing a collapsed card opens it again, where it was.
    work.show('terminal');
    expect(work.open()[1]).toEqual({ id: 'terminal', collapsed: false });

    work.maximize('knowledge');
    expect(work.visible().map((card) => card.id)).toEqual(['knowledge']);
    work.maximize('knowledge');
    expect(work.visible().length).toBe(3);

    work.move('knowledge', -1);
    expect(work.open().map((card) => card.id)).toEqual(['review', 'knowledge', 'terminal']);
    work.close('review');
    expect(JSON.parse(localStorage.getItem('pwr:workbench')!).open.map((card: any) => card.id)).toEqual([
      'knowledge',
      'terminal',
    ]);
  });

  it('offers only what needs no workspace in chat mode, and keeps the rest for later', () => {
    const work = TestBed.inject(WorkbenchStore);
    const agent = TestBed.inject(AgentStore);
    work.show('terminal');
    work.show('activity');
    work.maximize('terminal');
    agent.chatHome.set('/home/chat');
    agent.workspace.set('/home/chat');
    expect(work.available().map((card) => card.id)).toEqual(['browser', 'knowledge', 'activity']);
    expect(work.visible().map((card) => card.id)).toEqual(['activity']);
    expect(work.focused()).toBeNull();
    work.show('files');
    expect(work.isOpen('files')).toBe(false);
    // Back in a workspace, the column is as it was left.
    agent.workspace.set('/projects/app');
    expect(work.visible().map((card) => card.id)).toEqual(['terminal']);
  });

  it('asks the Files card for a file and opens it', () => {
    const work = TestBed.inject(WorkbenchStore);
    work.openFile('src/app.ts');
    expect(work.fileRequest()).toBe('src/app.ts');
    expect(work.isOpen('files')).toBe(true);
  });
});

describe('the Web preview card', () => {
  it('previews only pages on this machine', () => {
    expect(normalize('localhost:4200')).toBe('http://localhost:4200/');
    expect(normalize('http://127.0.0.1:8000/docs')).toBe('http://127.0.0.1:8000/docs');
    expect(normalize('https://example.com')).toBeNull();
    expect(normalize('file:///etc/passwd')).toBeNull();
    expect(normalize('javascript:alert(1)')).toBeNull();
    expect(normalize('')).toBeNull();
    // Only what the frame's policy admits.
    expect(normalize('http://[::1]:3000')).toBeNull();
    // The app's own origin is never framed: same-origin scripts would reach it.
    expect(normalize('localhost:4200', 'http://localhost:4200')).toBeNull();
    expect(normalize('localhost:4201', 'http://localhost:4200')).toBe('http://localhost:4201/');
  });
});
