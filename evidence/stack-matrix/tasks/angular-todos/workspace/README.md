# todos

An Angular 21 app (standalone components, signals, zoneless, Vitest). Build
the todo list: a store and a component, in `src/app/`.

## `TodoStore` (`src/app/todo-store.ts`)

An `@Injectable({ providedIn: 'root' })` service holding the todos in
signals.

```ts
export interface Todo { id: number; title: string; done: boolean }
export type Filter = 'all' | 'active' | 'done';

class TodoStore {
  readonly todos: Signal<Todo[]>;        // in the order added
  readonly filter: Signal<Filter>;       // starts 'all'
  readonly visible: Signal<Todo[]>;      // todos passing the filter
  readonly remaining: Signal<number>;    // todos not done
  add(title: string): void;              // trimmed; ignored when empty; ids 1, 2, 3... never reused
  toggle(id: number): void;
  remove(id: number): void;
  clearDone(): void;
  setFilter(filter: Filter): void;
}
```

It persists to `localStorage` under the key `todos` as
`{"todos": [...], "nextId": n}` whenever the todos change, and a new store
starts from what is saved there (an absent or unreadable value means an
empty list).

## `TodoList` (`src/app/todo-list.ts`, selector `app-todo-list`)

A standalone component showing the store:

- A text input labelled `New todo`; pressing Enter adds its value and
  clears the input.
- One `<li>` per visible todo, containing a checkbox whose label is the
  todo's title (checked when done; changing it toggles the todo) and a
  button labelled `Delete <title>` (its accessible name, e.g. via
  `aria-label`).
- Three buttons `All`, `Active`, `Done` that set the filter; the current one
  has `aria-pressed="true"`, the others `"false"`.
- A status (`role="status"`) reading `N items left`, `1 item left` for one.
- A button `Clear done`, disabled when nothing is done.

Run the tests with `npm test -- --watch=false` after `npm install`.
