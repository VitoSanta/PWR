import { TestBed } from '@angular/core/testing';
import { TodoStore } from './todo-store';
import { TodoList } from './todo-list';

describe('hidden', () => {
  beforeEach(() => localStorage.clear());

  it('an unreadable saved value starts empty', () => {
    localStorage.setItem('todos', '{not json');
    const s = TestBed.inject(TodoStore);
    expect(s.todos()).toEqual([]);
    s.add('x');
    expect(s.todos()[0].id).toBe(1);
  });

  it('ids are not reused after removing the last todo, across a restore', () => {
    const s = TestBed.inject(TodoStore);
    s.add('a');
    s.add('b');
    s.remove(2);
    TestBed.tick();
    TestBed.resetTestingModule();
    const again = TestBed.inject(TodoStore);
    again.add('c');
    expect(again.todos().map((t) => t.id)).toEqual([1, 3]);
  });

  it('zero items left, and unknown ids are harmless', () => {
    const s = TestBed.inject(TodoStore);
    s.add('only');
    s.toggle(1);
    s.toggle(99);
    s.remove(99);
    expect(s.remaining()).toBe(0);
    expect(s.visible()).toHaveLength(1);
  });

  it('the component shows 0 items left and the pressed default filter', async () => {
    await TestBed.configureTestingModule({ imports: [TodoList] }).compileComponents();
    const fixture = TestBed.createComponent(TodoList);
    await fixture.whenStable();
    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelector('[role="status"]')!.textContent!.trim()).toBe('0 items left');
    const all = [...root.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'All')!;
    expect(all.getAttribute('aria-pressed')).toBe('true');
  });
});
