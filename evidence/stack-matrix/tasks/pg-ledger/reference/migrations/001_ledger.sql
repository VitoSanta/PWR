CREATE TABLE accounts (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  code text NOT NULL UNIQUE CHECK (code ~ '^[A-Z0-9-]{3,20}$'),
  name text NOT NULL,
  kind text NOT NULL CHECK (kind IN ('asset', 'liability', 'equity', 'income', 'expense'))
);

CREATE TABLE transactions (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  occurred_on date NOT NULL,
  memo text NOT NULL DEFAULT '',
  metadata jsonb NOT NULL DEFAULT '{}'
);

CREATE TABLE entries (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  transaction_id bigint NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
  account_id bigint NOT NULL REFERENCES accounts (id),
  amount_cents bigint NOT NULL CHECK (amount_cents <> 0)
);

CREATE INDEX entries_transaction ON entries (transaction_id);
CREATE INDEX entries_account ON entries (account_id);

CREATE FUNCTION check_balanced(p_transaction bigint) RETURNS void LANGUAGE plpgsql AS $$
DECLARE total bigint; lines int;
BEGIN
  IF NOT EXISTS (SELECT 1 FROM transactions WHERE id = p_transaction) THEN
    RETURN;
  END IF;
  SELECT COALESCE(SUM(amount_cents), 0), count(*) INTO total, lines FROM entries WHERE transaction_id = p_transaction;
  IF total <> 0 OR lines < 2 THEN
    RAISE EXCEPTION 'unbalanced transaction %: % entries summing to %', p_transaction, lines, total;
  END IF;
END $$;

CREATE FUNCTION entries_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP IN ('UPDATE', 'DELETE') THEN
    PERFORM check_balanced(OLD.transaction_id);
  END IF;
  IF TG_OP IN ('INSERT', 'UPDATE') THEN
    PERFORM check_balanced(NEW.transaction_id);
  END IF;
  RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER entries_balanced
  AFTER INSERT OR UPDATE OR DELETE ON entries
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION entries_balanced();

CREATE FUNCTION transactions_have_entries() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM check_balanced(NEW.id);
  RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER transactions_have_entries
  AFTER INSERT ON transactions
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION transactions_have_entries();

CREATE VIEW account_balances AS
  SELECT a.code, a.name, a.kind, COALESCE(SUM(e.amount_cents), 0)::bigint AS balance_cents
  FROM accounts a
  LEFT JOIN entries e ON e.account_id = a.id
  GROUP BY a.id
  ORDER BY a.code;

CREATE FUNCTION statement(p_code text, p_from date, p_to date)
RETURNS TABLE (occurred_on date, transaction_id bigint, memo text, amount_cents bigint, running_balance_cents bigint)
LANGUAGE plpgsql STABLE AS $$
#variable_conflict use_column
DECLARE account bigint; opening bigint;
BEGIN
  SELECT a.id INTO account FROM accounts a WHERE a.code = p_code;
  IF account IS NULL THEN
    RAISE EXCEPTION 'unknown account %', p_code;
  END IF;
  SELECT COALESCE(SUM(e.amount_cents), 0) INTO opening
  FROM entries e JOIN transactions t ON t.id = e.transaction_id
  WHERE e.account_id = account AND t.occurred_on < p_from;
  RETURN QUERY
    SELECT t.occurred_on, t.id, t.memo, e.amount_cents,
           (opening + SUM(e.amount_cents) OVER (ORDER BY t.occurred_on, t.id, e.id))::bigint
    FROM entries e JOIN transactions t ON t.id = e.transaction_id
    WHERE e.account_id = account AND t.occurred_on BETWEEN p_from AND p_to
    ORDER BY t.occurred_on, t.id, e.id;
END $$;
