import { Injectable, computed, effect, signal } from '@angular/core';

export interface Todo {
  id: number;
  title: string;
  done: boolean;
}

export type Filter = 'all' | 'active' | 'done';

const KEY = 'todos';

function load(): { todos: Todo[]; nextId: number } {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (saved && Array.isArray(saved.todos) && typeof saved.nextId === 'number') {
      return saved;
    }
  } catch {
    // An unreadable value is an empty list.
  }
  return { todos: [], nextId: 1 };
}

@Injectable({ providedIn: 'root' })
export class TodoStore {
  private readonly state = signal(load());
  private readonly currentFilter = signal<Filter>('all');

  readonly todos = computed(() => this.state().todos);
  readonly filter = this.currentFilter.asReadonly();
  readonly visible = computed(() => {
    const filter = this.currentFilter();
    return this.todos().filter((t) => filter === 'all' || (filter === 'done' ? t.done : !t.done));
  });
  readonly remaining = computed(() => this.todos().filter((t) => !t.done).length);

  constructor() {
    effect(() => localStorage.setItem(KEY, JSON.stringify(this.state())));
  }

  add(title: string): void {
    const trimmed = title.trim();
    if (!trimmed) return;
    this.state.update(({ todos, nextId }) => ({
      todos: [...todos, { id: nextId, title: trimmed, done: false }],
      nextId: nextId + 1,
    }));
  }

  toggle(id: number): void {
    this.state.update((s) => ({ ...s, todos: s.todos.map((t) => (t.id === id ? { ...t, done: !t.done } : t)) }));
  }

  remove(id: number): void {
    this.state.update((s) => ({ ...s, todos: s.todos.filter((t) => t.id !== id) }));
  }

  clearDone(): void {
    this.state.update((s) => ({ ...s, todos: s.todos.filter((t) => !t.done) }));
  }

  setFilter(filter: Filter): void {
    this.currentFilter.set(filter);
  }
}
