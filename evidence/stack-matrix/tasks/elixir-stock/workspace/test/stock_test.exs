defmodule StockTest do
  use ExUnit.Case, async: true

  setup do
    {:ok, clock} = Agent.start_link(fn -> 1_000 end)
    {:ok, stock} = Stock.start_link(now: fn -> Agent.get(clock, & &1) end)
    %{stock: stock, clock: clock}
  end

  defp advance(clock, ms), do: Agent.update(clock, &(&1 + ms))

  test "adds and reports stock", %{stock: s} do
    assert {:ok, 5} = Stock.add(s, "A", 5)
    assert {:ok, 8} = Stock.add(s, "A", 3)
    assert Stock.on_hand(s, "A") == 8
    assert Stock.available(s, "missing") == 0
    assert {:error, :invalid_quantity} = Stock.add(s, "A", 0)
    assert {:error, :invalid_quantity} = Stock.add(s, "A", -2)
  end

  test "reserves, confirms and cancels", %{stock: s} do
    Stock.add(s, "A", 10)
    {:ok, r1} = Stock.reserve(s, "A", 4, 60_000)
    {:ok, r2} = Stock.reserve(s, "A", 5, 60_000)
    assert r1 != r2
    assert Stock.available(s, "A") == 1
    assert {:error, :insufficient_stock} = Stock.reserve(s, "A", 2, 60_000)
    assert :ok = Stock.confirm(s, r1)
    assert Stock.on_hand(s, "A") == 6
    assert Stock.available(s, "A") == 1
    assert :ok = Stock.cancel(s, r2)
    assert Stock.available(s, "A") == 6
    assert {:error, :not_found} = Stock.confirm(s, r1)
    assert {:error, :not_found} = Stock.cancel(s, r2)
  end

  test "errors on unknown SKUs and bad quantities", %{stock: s} do
    assert {:error, :unknown_sku} = Stock.reserve(s, "nope", 1, 1_000)
    Stock.add(s, "A", 1)
    assert {:error, :invalid_quantity} = Stock.reserve(s, "A", 0, 1_000)
  end

  test "reservations expire", %{stock: s, clock: c} do
    Stock.add(s, "A", 3)
    {:ok, id} = Stock.reserve(s, "A", 3, 5_000)
    advance(c, 4_999)
    assert Stock.available(s, "A") == 0
    advance(c, 1)
    assert Stock.available(s, "A") == 3
    assert {:error, :expired} = Stock.confirm(s, id)
    assert Stock.on_hand(s, "A") == 3
  end
end
