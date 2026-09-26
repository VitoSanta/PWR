\set ON_ERROR_STOP 1
BEGIN;
INSERT INTO accounts (code, name, kind) VALUES ('CASH', 'Cash', 'asset'), ('SALES-EU', 'Sales EU', 'income');

DO $$
DECLARE
  bad text[] := ARRAY[
    $q$INSERT INTO accounts (code, name, kind) VALUES ('cash', 'lower', 'asset')$q$,
    $q$INSERT INTO accounts (code, name, kind) VALUES ('AB', 'short', 'asset')$q$,
    $q$INSERT INTO accounts (code, name, kind) VALUES ('CASH', 'dup', 'asset')$q$,
    $q$INSERT INTO accounts (code, name, kind) VALUES ('OTHER', 'kind', 'revenue')$q$,
    $q$INSERT INTO accounts (code, name, kind) VALUES ('NONAME', NULL, 'asset')$q$
  ];
  statement text;
  accepted boolean;
BEGIN
  FOREACH statement IN ARRAY bad LOOP
    BEGIN
      EXECUTE statement;
      accepted := true;
    EXCEPTION WHEN others THEN
      accepted := false;
    END;
    IF accepted THEN RAISE EXCEPTION 'accepted: %', statement; END IF;
  END LOOP;
END $$;

DO $$
DECLARE t bigint;
BEGIN
  INSERT INTO transactions (occurred_on) VALUES ('2026-01-05') RETURNING id INTO t;
  IF (SELECT memo FROM transactions WHERE id = t) <> '' THEN RAISE EXCEPTION 'memo default'; END IF;
  IF (SELECT metadata FROM transactions WHERE id = t) <> '{}'::jsonb THEN RAISE EXCEPTION 'metadata default'; END IF;
  DECLARE accepted boolean;
  BEGIN
    BEGIN
      INSERT INTO entries (transaction_id, account_id, amount_cents)
        VALUES (t, (SELECT id FROM accounts WHERE code = 'CASH'), 0);
      accepted := true;
    EXCEPTION WHEN others THEN
      accepted := false;
    END;
    IF accepted THEN RAISE EXCEPTION 'zero amount accepted'; END IF;
  END;
END $$;
ROLLBACK;
