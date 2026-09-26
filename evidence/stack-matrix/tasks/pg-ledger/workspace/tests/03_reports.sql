\set ON_ERROR_STOP 1
BEGIN;
INSERT INTO accounts (code, name, kind) VALUES
  ('CASH', 'Cash', 'asset'), ('BANK', 'Bank', 'asset'), ('SALES', 'Sales', 'income'), ('RENT', 'Rent', 'expense');

CREATE FUNCTION pg_temp.post(on_date date, memo text, lines jsonb) RETURNS bigint AS $$
DECLARE t bigint; line jsonb;
BEGIN
  INSERT INTO transactions (occurred_on, memo) VALUES (on_date, memo) RETURNING id INTO t;
  FOR line IN SELECT * FROM jsonb_array_elements(lines) LOOP
    INSERT INTO entries (transaction_id, account_id, amount_cents)
      VALUES (t, (SELECT id FROM accounts WHERE code = line->>0), (line->>1)::bigint);
  END LOOP;
  RETURN t;
END $$ LANGUAGE plpgsql;

SELECT pg_temp.post('2026-01-02', 'opening sale', '[["CASH", 5000], ["SALES", -5000]]');
SELECT pg_temp.post('2026-02-01', 'rent', '[["RENT", 3000], ["CASH", -3000]]');
SELECT pg_temp.post('2026-02-10', 'deposit', '[["BANK", 1500], ["CASH", -1500]]');
SELECT pg_temp.post('2026-02-10', 'sale', '[["CASH", 2500], ["SALES", -2500]]');
SELECT pg_temp.post('2026-03-01', 'late sale', '[["CASH", 100], ["SALES", -100]]');
SET CONSTRAINTS ALL IMMEDIATE;

DO $$
DECLARE got text;
BEGIN
  SELECT string_agg(format('%s:%s', code, balance_cents), ' ' ORDER BY code) INTO got FROM account_balances;
  IF got <> 'BANK:1500 CASH:3100 RENT:3000 SALES:-7600' THEN RAISE EXCEPTION 'account_balances: %', got; END IF;

  SELECT string_agg(format('%s|%s|%s|%s', occurred_on, memo, amount_cents, running_balance_cents), ' ; ')
    INTO got FROM statement('CASH', '2026-02-01', '2026-02-28');
  IF got <> '2026-02-01|rent|-3000|2000 ; 2026-02-10|deposit|-1500|500 ; 2026-02-10|sale|2500|3000' THEN
    RAISE EXCEPTION 'statement: %', got;
  END IF;

  BEGIN
    PERFORM * FROM statement('NOPE', '2026-01-01', '2026-12-31');
    RAISE EXCEPTION 'TEST FAILED: unknown account accepted';
  EXCEPTION WHEN others THEN
    IF SQLERRM NOT LIKE '%unknown account%' THEN RAISE EXCEPTION 'wrong error: %', SQLERRM; END IF;
  END;
END $$;
ROLLBACK;
