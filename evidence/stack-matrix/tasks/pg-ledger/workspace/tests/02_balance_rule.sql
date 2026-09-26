\set ON_ERROR_STOP 1
BEGIN;
INSERT INTO accounts (code, name, kind) VALUES ('CASH', 'Cash', 'asset'), ('SALES', 'Sales', 'income');

-- A balanced transaction, entry by entry, commits.
DO $$
DECLARE t bigint;
BEGIN
  INSERT INTO transactions (occurred_on, memo) VALUES ('2026-01-05', 'sale') RETURNING id INTO t;
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES (t, (SELECT id FROM accounts WHERE code = 'CASH'), 1500);
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES (t, (SELECT id FROM accounts WHERE code = 'SALES'), -1500);
  SET CONSTRAINTS ALL IMMEDIATE;
  SET CONSTRAINTS ALL DEFERRED;
END $$;

-- An unbalanced one fails when checked.
DO $$
DECLARE t bigint;
BEGIN
  INSERT INTO transactions (occurred_on) VALUES ('2026-01-06') RETURNING id INTO t;
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES (t, (SELECT id FROM accounts WHERE code = 'CASH'), 1000);
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES (t, (SELECT id FROM accounts WHERE code = 'SALES'), -999);
  BEGIN
    SET CONSTRAINTS ALL IMMEDIATE;
    RAISE EXCEPTION 'TEST FAILED: unbalanced accepted';
  EXCEPTION WHEN others THEN
    IF SQLERRM LIKE 'TEST FAILED%' THEN RAISE; END IF;
    IF SQLERRM NOT LIKE '%unbalanced transaction%' THEN RAISE EXCEPTION 'TEST FAILED: wrong error: %', SQLERRM; END IF;
  END;
END $$;
ROLLBACK;

BEGIN;
INSERT INTO accounts (code, name, kind) VALUES ('CASH', 'Cash', 'asset');
-- A single entry never balances.
DO $$
DECLARE t bigint;
BEGIN
  INSERT INTO transactions (occurred_on) VALUES ('2026-01-07') RETURNING id INTO t;
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES (t, (SELECT id FROM accounts WHERE code = 'CASH'), 700);
  BEGIN
    SET CONSTRAINTS ALL IMMEDIATE;
    RAISE EXCEPTION 'TEST FAILED: single entry accepted';
  EXCEPTION WHEN others THEN
    IF SQLERRM LIKE 'TEST FAILED%' THEN RAISE; END IF;
  END;
END $$;
ROLLBACK;
