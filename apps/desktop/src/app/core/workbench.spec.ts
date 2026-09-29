import { TestBed } from '@angular/core/testing';
import { normalize } from '../ui/workbench/cards';
import { AgentStore } from './agent.store';
import { LayoutService, MAIN_MIN, RIGHT } from './layout';
import { CARD_GAP, WorkbenchStore } from './workbench';

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

describe('the Focus grid', () => {
  beforeEach(() => {
    for (const key of ['pwr:workbench', 'pwr:layout', 'pwr:card-widths', 'pwr:grid-width', 'pwr:card-splits']) localStorage.removeItem(key);
    TestBed.configureTestingModule({});
  });

  const setup = (viewport: number) => {
    const work = TestBed.inject(WorkbenchStore);
    const layout = TestBed.inject(LayoutService);
    layout.viewport.set(viewport);
    layout.leftFixed.set(0);
    work.focusMode.set(true);
    return { work, layout };
  };
  const two = 2 * RIGHT.initial + CARD_GAP;

  it('widens the grid for a second tool, and puts the two side by side', () => {
    const { work, layout } = setup(1600);
    expect(work.columnWidth()).toBe(RIGHT.initial);
    work.show('files');
    expect(work.columnWidth()).toBe(two);
    expect(work.rows()).toEqual([['review', 'files']]);
    expect(work.widthOf('review') + CARD_GAP + work.widthOf('files')).toBe(two);
    TestBed.tick();
    expect(layout.rightWidth()).toBe(two);
    // An odd one out takes the whole row.
    work.show('terminal');
    expect(work.rows()).toEqual([['review', 'files'], ['terminal']]);
    expect(work.widthOf('terminal')).toBe(two);
  });

  it('stacks the tools where the window has no room for two', () => {
    const { work } = setup(1100);
    work.show('files');
    expect(work.rows()).toEqual([['review'], ['files']]);
    expect(work.widthOf('files')).toBe(RIGHT.initial);
  });

  it('gives a wide tool a row of its own, and a collapsed one too', () => {
    const { work } = setup(1600);
    work.show('files');
    work.show('terminal');
    work.toggleWide('files');
    expect(work.rows()).toEqual([['review'], ['files'], ['terminal']]);
    work.toggleWide('files');
    work.collapse('review');
    expect(work.rows()).toEqual([['review'], ['files', 'terminal']]);
  });

  it('moves the line between two cards side by side, and the grid stays', () => {
    const { work } = setup(1600);
    work.show('files');
    work.resize('files', 400);
    expect(work.widthOf('files')).toBe(400);
    expect(work.widthOf('review')).toBe(two - CARD_GAP - 400);
    expect(work.columnWidth()).toBe(two);
    // Never past the other card's least.
    work.resize('files', 5000);
    expect(work.widthOf('review')).toBe(RIGHT.min);
  });

  it('sizes the whole grid from its outer edge, every row with it', () => {
    const { work } = setup(1600);
    work.show('files');
    work.show('terminal');
    const right = work.widthOf('files');
    work.resize('terminal', 900);
    expect(work.columnWidth()).toBe(900);
    expect(work.widthOf('terminal')).toBe(900);
    // The pair keeps its right card; the left one takes the difference.
    expect(work.widthOf('files')).toBe(right);
    expect(work.widthOf('review')).toBe(900 - CARD_GAP - right);
    // From a pair's left card, the same.
    work.resize('review', 500);
    expect(work.columnWidth()).toBe(500 + CARD_GAP + right);
    // Never so wide the conversation loses its least width.
    work.resize('terminal', 5000);
    expect(work.columnWidth()).toBe(1600 - MAIN_MIN);
    // Too narrow for two, they stack.
    work.resize('terminal', RIGHT.min);
    expect(work.rows()).toEqual([['review'], ['files'], ['terminal']]);
  });

  it('keeps the grid and the splits for the next launch', () => {
    const { work } = setup(1600);
    work.show('files');
    work.resize('files', 400);
    expect(JSON.parse(localStorage.getItem('pwr:grid-width')!)).toBe(two);
    expect(JSON.parse(localStorage.getItem('pwr:card-splits')!).review).toBeCloseTo((two - CARD_GAP - 400) / (two - CARD_GAP));
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
