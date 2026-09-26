\set ON_ERROR_STOP 1
BEGIN;
INSERT INTO accounts (code, name, kind) VALUES ('CASH', 'Cash', 'asset'), ('SALES', 'Sales', 'income'), ('IDLE', 'Idle', 'equity');

DO $$
DECLARE t bigint; got text; accepted boolean;
BEGIN
  INSERT INTO transactions (occurred_on, memo, metadata) VALUES ('2026-05-01', 'split', '{"invoice": 7}') RETURNING id INTO t;
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES
    (t, (SELECT id FROM accounts WHERE code = 'CASH'), 300),
    (t, (SELECT id FROM accounts WHERE code = 'CASH'), 200),
    (t, (SELECT id FROM accounts WHERE code = 'SALES'), -500);
  SET CONSTRAINTS ALL IMMEDIATE;
  SET CONSTRAINTS ALL DEFERRED;

  -- An account nobody posted to shows 0.
  SELECT balance_cents::text INTO got FROM account_balances WHERE code = 'IDLE';
  IF got <> '0' THEN RAISE EXCEPTION 'idle balance: %', got; END IF;

  -- Two entries of one transaction: two statement lines, running in entry order.
  SELECT string_agg(running_balance_cents::text, ',') INTO got FROM statement('CASH', '2026-05-01', '2026-05-01');
  IF got <> '300,500' THEN RAISE EXCEPTION 'statement: %', got; END IF;

  -- An empty period is an empty statement, not an error.
  IF EXISTS (SELECT 1 FROM statement('CASH', '2027-01-01', '2027-12-31')) THEN RAISE EXCEPTION 'empty period'; END IF;

  -- An account with entries cannot be deleted.
  BEGIN
    DELETE FROM accounts WHERE code = 'CASH';
    accepted := true;
  EXCEPTION WHEN others THEN accepted := false;
  END;
  IF accepted THEN RAISE EXCEPTION 'deleted an account with entries'; END IF;

  -- Deleting the transaction takes its entries with it, and stays balanced.
  DELETE FROM transactions WHERE id = t;
  SET CONSTRAINTS ALL IMMEDIATE;
  SET CONSTRAINTS ALL DEFERRED;
  IF EXISTS (SELECT 1 FROM entries) THEN RAISE EXCEPTION 'entries left behind'; END IF;

  -- An update that unbalances is caught too.
  INSERT INTO transactions (occurred_on) VALUES ('2026-05-02') RETURNING id INTO t;
  INSERT INTO entries (transaction_id, account_id, amount_cents) VALUES
    (t, (SELECT id FROM accounts WHERE code = 'CASH'), 100),
    (t, (SELECT id FROM accounts WHERE code = 'SALES'), -100);
  SET CONSTRAINTS ALL IMMEDIATE;
  SET CONSTRAINTS ALL DEFERRED;
  UPDATE entries SET amount_cents = 101 WHERE amount_cents = 100;
  BEGIN
    SET CONSTRAINTS ALL IMMEDIATE;
    accepted := true;
  EXCEPTION WHEN others THEN accepted := false;
  END;
  IF accepted THEN RAISE EXCEPTION 'unbalancing update accepted'; END IF;
END $$;
ROLLBACK;

BEGIN;
-- A transaction with no entries at all does not commit.
DO $$
DECLARE accepted boolean;
BEGIN
  INSERT INTO transactions (occurred_on) VALUES ('2026-06-01');
  BEGIN
    SET CONSTRAINTS ALL IMMEDIATE;
    accepted := true;
  EXCEPTION WHEN others THEN accepted := false;
  END;
  IF accepted THEN RAISE EXCEPTION 'empty transaction accepted'; END IF;
END $$;
ROLLBACK;
