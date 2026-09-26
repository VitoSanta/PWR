import { describe, expect, it, afterEach } from 'vitest';
import { render, screen, within, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DataTable, type Column } from '../src/DataTable';

afterEach(cleanup);

interface Item { id: string; label: string; size: number; }

const items: Item[] = [
  { id: 'a', label: 'same', size: 10 },
  { id: 'b', label: 'same', size: 9 },
  { id: 'c', label: 'other', size: 100 },
  { id: 'd', label: 'same', size: 9 },
];

const columns: Column<Item>[] = [
  { key: 'label', header: 'Label', sortable: true },
  { key: 'size', header: 'Size', sortable: true, render: (item) => <strong>{item.size} kB</strong> },
];

const rows = () => within(screen.getAllByRole('rowgroup')[1]).getAllByRole('row');
const ids = () => rows().map((row) => row.getAttribute('data-id') ?? within(row).getAllByRole('cell')[1].textContent);

describe('DataTable (hidden)', () => {
  it('sorts numbers as numbers and uses render', async () => {
    const user = userEvent.setup();
    render(<DataTable rows={items} columns={columns} getRowId={(i) => i.id} />);
    await user.click(screen.getByRole('button', { name: 'Size' }));
    expect(rows().map((row) => within(row).getAllByRole('cell')[1].textContent)).toEqual(['9 kB', '9 kB', '10 kB', '100 kB']);
    expect(screen.getAllByRole('columnheader')[1].querySelector('strong')).toBeNull();
    expect(within(rows()[0]).getAllByRole('cell')[1].querySelector('strong')).not.toBeNull();
  });

  it('keeps equal rows in their original order both ways', async () => {
    const user = userEvent.setup();
    render(<DataTable rows={items} columns={columns} getRowId={(i) => i.id} />);
    await user.click(screen.getByRole('button', { name: 'Label' }));
    expect(rows().map((row) => within(row).getAllByRole('cell')[1].textContent)).toEqual(['100 kB', '10 kB', '9 kB', '9 kB']);
    await user.click(screen.getByRole('button', { name: 'Label' }));
    expect(rows().map((row) => within(row).getAllByRole('cell')[1].textContent)).toEqual(['10 kB', '9 kB', '9 kB', '100 kB']);
  });

  it('switching column starts ascending and resets the other', async () => {
    const user = userEvent.setup();
    render(<DataTable rows={items} columns={columns} getRowId={(i) => i.id} />);
    await user.click(screen.getByRole('button', { name: 'Label' }));
    await user.click(screen.getByRole('button', { name: 'Label' }));
    await user.click(screen.getByRole('button', { name: 'Size' }));
    const [label, size] = screen.getAllByRole('columnheader');
    expect(label.getAttribute('aria-sort')).toBe('none');
    expect(size.getAttribute('aria-sort')).toBe('ascending');
  });

  it('one row is singular and the page size is honoured', () => {
    render(<DataTable rows={items.slice(0, 1)} columns={columns} getRowId={(i) => i.id} pageSize={1} />);
    expect(screen.getByRole('status').textContent).toBe('1 row · Page 1 of 1');
    expect(screen.getByRole('button', { name: 'Next' })).toHaveProperty('disabled', true);
  });

  it('a small page size pages the filtered rows', async () => {
    const user = userEvent.setup();
    render(<DataTable rows={items} columns={columns} getRowId={(i) => i.id} pageSize={2} />);
    expect(screen.getByRole('status').textContent).toBe('4 rows · Page 1 of 2');
    await user.type(screen.getByRole('searchbox', { name: 'Filter' }), 'same');
    expect(screen.getByRole('status').textContent).toBe('3 rows · Page 1 of 2');
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(rows()).toHaveLength(1);
  });
});
