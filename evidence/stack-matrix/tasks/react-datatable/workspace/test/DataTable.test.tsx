import { describe, expect, it } from 'vitest';
import { render, screen, within, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach } from 'vitest';
import { DataTable, type Column } from '../src/DataTable';
import { users, type User } from './fixtures';

afterEach(cleanup);

const columns: Column<User>[] = [
  { key: 'name', header: 'Name', sortable: true },
  { key: 'age', header: 'Age', sortable: true },
  { key: 'city', header: 'City' },
];

function setup(rows = users, pageSize?: number) {
  render(<DataTable rows={rows} columns={columns} getRowId={(u) => u.id} pageSize={pageSize} />);
  return userEvent.setup();
}

const bodyRows = () => within(screen.getAllByRole('rowgroup')[1]).getAllByRole('row');
const firstCells = () => bodyRows().map((row) => within(row).getAllByRole('cell')[0].textContent);

describe('DataTable', () => {
  it('shows the first page of ten rows and the status', () => {
    setup();
    expect(screen.getAllByRole('columnheader').map((th) => th.textContent)).toEqual(['Name', 'Age', 'City']);
    expect(bodyRows()).toHaveLength(10);
    expect(screen.getByRole('status').textContent).toBe('25 rows · Page 1 of 3');
    expect(screen.getByRole('button', { name: 'Previous' })).toHaveProperty('disabled', true);
  });

  it('moves between pages', async () => {
    const user = setup();
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(bodyRows()).toHaveLength(5);
    expect(screen.getByRole('status').textContent).toBe('25 rows · Page 3 of 3');
    expect(screen.getByRole('button', { name: 'Next' })).toHaveProperty('disabled', true);
    await user.click(screen.getByRole('button', { name: 'Previous' }));
    expect(screen.getByRole('status').textContent).toBe('25 rows · Page 2 of 3');
  });

  it('sorts ascending, descending, then back to the original order', async () => {
    const user = setup(users.slice(0, 5));
    const header = () => screen.getAllByRole('columnheader')[0];
    await user.click(screen.getByRole('button', { name: 'Name' }));
    expect(firstCells()).toEqual(['Ada', 'bruno', 'luca', 'Mara', 'Zoe']);
    expect(header().getAttribute('aria-sort')).toBe('ascending');
    await user.click(screen.getByRole('button', { name: 'Name' }));
    expect(firstCells()).toEqual(['Zoe', 'Mara', 'luca', 'bruno', 'Ada']);
    expect(header().getAttribute('aria-sort')).toBe('descending');
    await user.click(screen.getByRole('button', { name: 'Name' }));
    expect(firstCells()).toEqual(['Mara', 'luca', 'Ada', 'Zoe', 'bruno']);
    expect(header().getAttribute('aria-sort')).toBe('none');
  });

  it('only sortable headers are buttons', () => {
    setup();
    expect(screen.queryByRole('button', { name: 'City' })).toBeNull();
    expect(screen.getAllByRole('columnheader')[1].getAttribute('aria-sort')).toBe('none');
  });

  it('filters on any column, ignoring case, and returns to page 1', async () => {
    const user = setup();
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await user.type(screen.getByRole('searchbox', { name: 'Filter' }), '  ROMA ');
    expect(screen.getByRole('status').textContent).toBe('8 rows · Page 1 of 1');
    expect(bodyRows().every((row) => row.textContent?.includes('Roma'))).toBe(true);
  });

  it('says so when nothing matches', async () => {
    const user = setup();
    await user.type(screen.getByRole('searchbox', { name: 'Filter' }), 'nobody');
    expect(bodyRows()).toHaveLength(1);
    const cell = within(bodyRows()[0]).getByRole('cell');
    expect(cell.textContent).toBe('No rows');
    expect(cell.getAttribute('colspan')).toBe('3');
    expect(screen.getByRole('status').textContent).toBe('0 rows · Page 1 of 1');
  });
});
