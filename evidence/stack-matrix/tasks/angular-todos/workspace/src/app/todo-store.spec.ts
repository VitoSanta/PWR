import { TestBed } from '@angular/core/testing';
import { TodoStore } from './todo-store';

describe('TodoStore', () => {
  beforeEach(() => localStorage.clear());

  const store = () => TestBed.inject(TodoStore);

  it('adds trimmed titles with increasing ids and ignores empty ones', () => {
    const s = store();
    s.add('  milk ');
    s.add('   ');
    s.add('bread');
    expect(s.todos()).toEqual([
      { id: 1, title: 'milk', done: false },
      { id: 2, title: 'bread', done: false },
    ]);
    expect(s.remaining()).toBe(2);
  });

  it('toggles, filters, removes and clears', () => {
    const s = store();
    ['a', 'b', 'c'].forEach((t) => s.add(t));
    s.toggle(2);
    expect(s.remaining()).toBe(2);
    s.setFilter('done');
    expect(s.visible().map((t) => t.title)).toEqual(['b']);
    s.setFilter('active');
    expect(s.visible().map((t) => t.title)).toEqual(['a', 'c']);
    s.clearDone();
    s.setFilter('all');
    expect(s.todos().map((t) => t.id)).toEqual([1, 3]);
    s.remove(1);
    s.add('d');
    expect(s.todos().map((t) => t.id)).toEqual([3, 4]);
  });

  it('persists and restores', () => {
    const first = store();
    first.add('keep');
    first.toggle(1);
    TestBed.tick();
    const saved = JSON.parse(localStorage.getItem('todos')!);
    expect(saved.todos).toEqual([{ id: 1, title: 'keep', done: true }]);
    TestBed.resetTestingModule();
    const second = TestBed.inject(TodoStore);
    expect(second.todos()).toEqual([{ id: 1, title: 'keep', done: true }]);
    second.add('next');
    expect(second.todos()[1].id).toBe(2);
  });
});
