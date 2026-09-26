# DataTable

A generic, accessible table component for the admin console: React 19 +
TypeScript, no UI library. It lives in `src/DataTable.tsx`.

```tsx
import { DataTable, type Column } from './DataTable';

<DataTable
  rows={users}
  getRowId={(user) => user.id}
  columns={[
    { key: 'name', header: 'Name', sortable: true },
    { key: 'age', header: 'Age', sortable: true },
    { key: 'email', header: 'Email', render: (user) => <a href={`mailto:${user.email}`}>{user.email}</a> },
  ]}
  pageSize={10}
/>
```

```ts
export interface Column<T> {
  key: keyof T & string;
  header: string;
  sortable?: boolean;
  render?: (row: T) => React.ReactNode;   // defaults to String(row[key])
}
export interface DataTableProps<T> {
  rows: T[];
  columns: Column<T>[];
  getRowId: (row: T) => string;
  pageSize?: number;                        // default 10
}
export function DataTable<T>(props: DataTableProps<T>): JSX.Element;
```

## Behaviour

- A `<table>` with one `<th>` per column in a `<thead>`, and one `<tr>` per
  visible row in the `<tbody>`, keyed by `getRowId`.
- **Filter**: a search box labelled `Filter` (`<input type="search"
  aria-label="Filter">`). A row is kept when the text of any column's value
  (`String(row[key])`) contains the query, ignoring case and surrounding
  spaces. Changing the filter goes back to page 1.
- **Sorting**: a sortable column's header text is a `<button>`. Clicking it
  sorts by that column ascending, again descending, a third time back to the
  original order; clicking another column starts ascending on that one. The
  `<th>` of the sorted column has `aria-sort="ascending"` or `"descending"`,
  every other sortable `<th>` `aria-sort="none"`. Numbers sort as numbers,
  everything else as strings with `localeCompare`. The sort is stable: rows
  that compare equal keep their original order.
- **Pages** of `pageSize` rows: buttons `Previous` and `Next` (disabled at
  the first and last page) and a status `Page X of Y` (`Y` is at least 1). A
  filter that leaves fewer pages than the current one also lands on page 1.
- `N rows` (the count after filtering; `1 row` for one) is shown in an
  element with `role="status"`, together with the page text:
  `12 rows · Page 1 of 2`.
- When no row matches, the body has a single row with one cell spanning all
  columns that says `No rows`.

Run the tests with `npm test`.
