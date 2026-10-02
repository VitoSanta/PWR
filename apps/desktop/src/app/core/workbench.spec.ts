import { TestBed } from '@angular/core/testing';
import { normalize } from '../ui/workbench/cards';
import { AgentStore } from './agent.store';
import { LayoutService } from './layout';
import { WorkbenchStore } from './workbench';

describe('WorkbenchStore', () => {
  beforeEach(() => {
    for (const key of ['pwr:inspector-tab', 'pwr:layout']) localStorage.removeItem(key);
    TestBed.configureTestingModule({});
  });

  const setup = (viewport = 1600) => {
    const layout = TestBed.inject(LayoutService);
    layout.viewport.set(viewport);
    return { work: TestBed.inject(WorkbenchStore), layout };
  };

  it('shows one tool at a time, and remembers which', () => {
    const { work } = setup();
    expect(work.active()).toBe('review');
    work.show('terminal');
    expect(work.active()).toBe('terminal');
    expect(work.isOpen('terminal')).toBe(true);
    expect(work.isOpen('review')).toBe(false);
    TestBed.tick();
    expect(localStorage.getItem('pwr:inspector-tab')).toBe('terminal');
  });

  it('opens the panel for a tool, and a shortcut closes it again', () => {
    const { work, layout } = setup();
    layout.rightOpen.set(false);
    expect(work.panelVisible()).toBe(false);
    work.toggle('files');
    expect(work.panelVisible()).toBe(true);
    expect(work.active()).toBe('files');
    work.toggle('files');
    expect(work.panelVisible()).toBe(false);
    // Another tool's shortcut switches to it instead.
    work.toggle('files');
    work.toggle('terminal');
    expect(work.panelVisible()).toBe(true);
    expect(work.active()).toBe('terminal');
  });

  it('floats the panel over the conversation where it cannot dock', () => {
    const { work, layout } = setup(900);
    work.show('plan');
    expect(layout.right()).toBe('overlay');
    expect(work.isOpen('plan')).toBe(true);
  });

  it('shows Knowledge and Activity as a tab only while they are open', () => {
    const { work } = setup();
    expect(work.tabs().map((card) => card.id)).toEqual(['review', 'terminal', 'browser', 'files', 'plan']);
    work.show('knowledge');
    expect(work.tabs().map((card) => card.id)).toContain('knowledge');
    work.close('knowledge');
    expect(work.active()).toBe('review');
    expect(work.tabs().map((card) => card.id)).not.toContain('knowledge');
  });

  it('offers only what needs no workspace in chat mode, and keeps the tab for later', () => {
    const { work } = setup();
    const agent = TestBed.inject(AgentStore);
    work.show('terminal');
    agent.chatHome.set('/home/chat');
    agent.workspace.set('/home/chat');
    expect(work.available().map((card) => card.id)).toEqual(['browser', 'knowledge', 'activity']);
    expect(work.active()).toBe('browser');
    work.show('files');
    expect(work.active()).toBe('browser');
    // Back in a workspace, the tab is as it was left.
    agent.workspace.set('/projects/app');
    expect(work.active()).toBe('terminal');
  });

  it('asks the Files tab for a file and opens it', () => {
    const { work } = setup();
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
