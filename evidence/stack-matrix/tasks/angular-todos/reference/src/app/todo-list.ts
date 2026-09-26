import { Component, inject } from '@angular/core';
import { Filter, TodoStore } from './todo-store';

@Component({
  selector: 'app-todo-list',
  template: `
    <label for="new-todo">New todo</label>
    <input id="new-todo" #field (keydown.enter)="store.add(field.value); field.value = ''" />
    <ul>
      @for (todo of store.visible(); track todo.id) {
        <li>
          <input type="checkbox" [id]="'todo-' + todo.id" [checked]="todo.done" (change)="store.toggle(todo.id)" />
          <label [for]="'todo-' + todo.id">{{ todo.title }}</label>
          <button type="button" [attr.aria-label]="'Delete ' + todo.title" (click)="store.remove(todo.id)">✕</button>
        </li>
      }
    </ul>
    @for (option of filters; track option.value) {
      <button type="button" [attr.aria-pressed]="store.filter() === option.value" (click)="store.setFilter(option.value)">
        {{ option.label }}
      </button>
    }
    <p role="status">{{ store.remaining() }} {{ store.remaining() === 1 ? 'item' : 'items' }} left</p>
    <button type="button" [disabled]="store.remaining() === store.todos().length" (click)="store.clearDone()">Clear done</button>
  `,
})
export class TodoList {
  protected readonly store = inject(TodoStore);
  protected readonly filters: { value: Filter; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'active', label: 'Active' },
    { value: 'done', label: 'Done' },
  ];
}
