defmodule StockHiddenTest do
  use ExUnit.Case, async: true

  setup do
    {:ok, clock} = Agent.start_link(fn -> 0 end)
    {:ok, stock} = Stock.start_link(now: fn -> Agent.get(clock, & &1) end)
    %{stock: stock, clock: clock}
  end

  test "an expired reservation cannot be cancelled either", %{stock: s, clock: c} do
    Stock.add(s, "A", 2)
    {:ok, id} = Stock.reserve(s, "A", 2, 10)
    Agent.update(c, &(&1 + 10))
    assert {:error, :not_found} = Stock.cancel(s, id)
  end

  test "expired stock can be reserved again", %{stock: s, clock: c} do
    Stock.add(s, "A", 2)
    {:ok, _} = Stock.reserve(s, "A", 2, 10)
    assert {:error, :insufficient_stock} = Stock.reserve(s, "A", 1, 10)
    Agent.update(c, &(&1 + 10))
    assert {:ok, _} = Stock.reserve(s, "A", 2, 10)
  end

  test "a named server", %{} do
    {:ok, _} = Stock.start_link(name: :hidden_stock, now: fn -> 0 end)
    assert {:ok, 4} = Stock.add(:hidden_stock, "B", 4)
    assert Stock.on_hand(:hidden_stock, "B") == 4
  end

  test "default clock works", %{} do
    {:ok, s} = Stock.start_link([])
    Stock.add(s, "C", 1)
    assert {:ok, _} = Stock.reserve(s, "C", 1, 60_000)
    assert Stock.available(s, "C") == 0
  end

  test "non-integer quantities are refused", %{stock: s} do
    assert {:error, :invalid_quantity} = Stock.add(s, "A", 1.5)
    Stock.add(s, "A", 3)
    assert {:error, :invalid_quantity} = Stock.reserve(s, "A", "2", 10)
  end

  test "many concurrent reservations never oversell", %{stock: s} do
    Stock.add(s, "HOT", 50)
    results =
      1..200
      |> Task.async_stream(fn _ -> Stock.reserve(s, "HOT", 1, 60_000) end, max_concurrency: 50)
      |> Enum.map(fn {:ok, result} -> result end)
    assert Enum.count(results, &match?({:ok, _}, &1)) == 50
    assert Stock.available(s, "HOT") == 0
  end
end
