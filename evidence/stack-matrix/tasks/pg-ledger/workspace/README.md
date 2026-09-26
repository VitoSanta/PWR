# ledger

A double-entry ledger in PostgreSQL 17. Everything lives in SQL migrations
under `migrations/` (applied in file-name order to an empty database); the
tests in `tests/` are plain SQL run with `psql`, each in a transaction it
rolls back.

## Tables

- `accounts`: `id bigint` generated identity primary key; `code text` unique,
  not null, matching `^[A-Z0-9-]{3,20}$`; `name text` not null; `kind text`
  not null, one of `asset`, `liability`, `equity`, `income`, `expense`.
- `transactions`: `id bigint` generated identity primary key; `occurred_on
  date` not null; `memo text` not null default `''`; `metadata jsonb` not
  null default `'{}'`.
- `entries`: `id bigint` generated identity primary key; `transaction_id`
  referencing `transactions` (deleting a transaction deletes its entries);
  `account_id` referencing `accounts` (an account with entries cannot be
  deleted); `amount_cents bigint` not null and not 0 (positive is a debit,
  negative a credit).

**Balance rule**: the entries of every transaction sum to zero, and a
transaction has at least two entries -- checked when the transaction
commits, so its entries can be inserted one at a time. A violation fails the
commit with an error whose message contains `unbalanced transaction`.

## Views and functions

- View `account_balances(code, name, kind, balance_cents)`: every account,
  with the sum of its entries (0 for none), ordered by `code`.
- Function `statement(p_code text, p_from date, p_to date)` returning table
  `(occurred_on date, transaction_id bigint, memo text, amount_cents bigint,
  running_balance_cents bigint)`: the account's entries dated from `p_from`
  to `p_to` inclusive, ordered by date then transaction id, each with the
  balance after it -- which counts everything before `p_from` as the opening
  balance. An unknown code raises an error whose message contains
  `unknown account`.

Run the tests with `sh scripts/test.sh` (it needs `psql` and either a server
in the `PG*` environment or Docker).
