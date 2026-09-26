import { useMemo, useState, type ReactNode } from 'react';

export interface Column<T> {
  key: keyof T & string;
  header: string;
  sortable?: boolean;
  render?: (row: T) => ReactNode;
}

export interface DataTableProps<T> {
  rows: T[];
  columns: Column<T>[];
  getRowId: (row: T) => string;
  pageSize?: number;
}

type Direction = 'ascending' | 'descending';

export function DataTable<T>({ rows, columns, getRowId, pageSize = 10 }: DataTableProps<T>) {
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<{ key: string; direction: Direction } | null>(null);
  const [page, setPage] = useState(1);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter((row) =>
      columns.some((column) => String(row[column.key]).toLowerCase().includes(needle)),
    );
  }, [rows, columns, query]);

  const sorted = useMemo(() => {
    if (!sort) return filtered;
    const factor = sort.direction === 'ascending' ? 1 : -1;
    return filtered
      .map((row, index) => ({ row, index }))
      .sort((a, b) => {
        const x = a.row[sort.key as keyof T];
        const y = b.row[sort.key as keyof T];
        const order =
          typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y));
        return order !== 0 ? order * factor : a.index - b.index;
      })
      .map(({ row }) => row);
  }, [filtered, sort]);

  const pages = Math.max(1, Math.ceil(sorted.length / pageSize));
  const current = Math.min(page, pages);
  const visible = sorted.slice((current - 1) * pageSize, current * pageSize);

  const toggle = (key: string) => {
    setSort((previous) => {
      if (!previous || previous.key !== key) return { key, direction: 'ascending' };
      if (previous.direction === 'ascending') return { key, direction: 'descending' };
      return null;
    });
  };

  return (
    <div>
      <input
        type="search"
        aria-label="Filter"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setPage(1);
        }}
      />
      <table>
        <thead>
          <tr>
            {columns.map((column) => (
              <th
                key={column.key}
                aria-sort={column.sortable ? (sort?.key === column.key ? sort.direction : 'none') : undefined}
              >
                {column.sortable ? (
                  <button type="button" onClick={() => toggle(column.key)}>
                    {column.header}
                  </button>
                ) : (
                  column.header
                )}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {visible.length === 0 ? (
            <tr>
              <td colSpan={columns.length}>No rows</td>
            </tr>
          ) : (
            visible.map((row) => (
              <tr key={getRowId(row)}>
                {columns.map((column) => (
                  <td key={column.key}>{column.render ? column.render(row) : String(row[column.key])}</td>
                ))}
              </tr>
            ))
          )}
        </tbody>
      </table>
      <div>
        <button type="button" disabled={current <= 1} onClick={() => setPage(current - 1)}>
          Previous
        </button>
        <span role="status">
          {sorted.length} {sorted.length === 1 ? 'row' : 'rows'} · Page {current} of {pages}
        </span>
        <button type="button" disabled={current >= pages} onClick={() => setPage(current + 1)}>
          Next
        </button>
      </div>
    </div>
  );
}
