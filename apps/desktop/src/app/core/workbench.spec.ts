import { TestBed } from '@angular/core/testing';
import { normalize } from '../ui/workbench/cards';
import { AgentStore } from './agent.store';
import { LayoutService, MAIN_MIN, RIGHT } from './layout';
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

  it('gives each Focus card its own width, and docks the column by the widest', () => {
    localStorage.setItem('pwr:card-widths', '{}');
    const work = TestBed.inject(WorkbenchStore);
    const layout = TestBed.inject(LayoutService);
    layout.viewport.set(1600);
    layout.leftFixed.set(72);
    work.focusMode.set(true);
    work.show('knowledge');
    work.setWidth('knowledge', 600);
    expect(work.widthOf('knowledge')).toBe(600);
    // Resizing one card leaves the others as they were.
    expect(work.widthOf('review')).toBe(RIGHT.initial);
    expect(work.columnWidth()).toBe(600);
    TestBed.tick();
    expect(layout.rightWidth()).toBe(600);
    // Never so wide the conversation loses its least width, never under a card's least.
    work.setWidth('knowledge', 5000);
    expect(work.widthOf('knowledge')).toBe(1600 - 72 - MAIN_MIN);
    work.setWidth('review', 10);
    expect(work.widthOf('review')).toBe(RIGHT.min);
    // A narrower window: the wide card gives way before the tools take the page.
    layout.viewport.set(1100);
    TestBed.tick();
    expect(layout.rightWidth()).toBe(1100 - 72 - MAIN_MIN);
    expect(JSON.parse(localStorage.getItem('pwr:card-widths')!)).toEqual({
      knowledge: 968,
      review: RIGHT.min,
    });
    // A card opened later takes the column's width, and resizing it moves no other.
    work.show('terminal');
    expect(work.widthOf('terminal')).toBe(968);
    work.setWidth('terminal', 500);
    expect(work.widthOf('knowledge')).toBe(968);
    expect(work.widthOf('review')).toBe(RIGHT.min);
  });

  it('keeps the width the column had for the cards already open, the first time', () => {
    localStorage.removeItem('pwr:card-widths');
    localStorage.setItem(
      'pwr:workbench',
      JSON.stringify({ open: [{ id: 'files', collapsed: false }], maximized: null }),
    );
    localStorage.setItem(
      'pwr:layout',
      JSON.stringify({ leftOpen: true, rightOpen: true, leftWidth: 288, rightWidth: 400 }),
    );
    const work = TestBed.inject(WorkbenchStore);
    expect(work.widthOf('files')).toBe(400);
    // A card opened later follows it, rather than the default.
    expect(work.widthOf('terminal')).toBe(400);
    localStorage.removeItem('pwr:layout');
  });

  it('keeps multiple tools open in Focus', () => {
    const work = TestBed.inject(WorkbenchStore);
    work.focusMode.set(true);
    work.show('activity');
    work.show('knowledge');
    expect(work.visible().map((card) => card.id)).toEqual(['review', 'activity', 'knowledge']);
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
