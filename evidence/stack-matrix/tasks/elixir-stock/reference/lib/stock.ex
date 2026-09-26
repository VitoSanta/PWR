defmodule Stock do
  @moduledoc "Warehouse stock with reservations that expire."
  use GenServer

  def start_link(opts) do
    now = Keyword.get(opts, :now, fn -> System.monotonic_time(:millisecond) end)
    case Keyword.fetch(opts, :name) do
      {:ok, name} -> GenServer.start_link(__MODULE__, now, name: name)
      :error -> GenServer.start_link(__MODULE__, now)
    end
  end

  def add(server, sku, qty), do: GenServer.call(server, {:add, sku, qty})
  def reserve(server, sku, qty, ttl_ms), do: GenServer.call(server, {:reserve, sku, qty, ttl_ms})
  def confirm(server, id), do: GenServer.call(server, {:confirm, id})
  def cancel(server, id), do: GenServer.call(server, {:cancel, id})
  def available(server, sku), do: GenServer.call(server, {:available, sku})
  def on_hand(server, sku), do: GenServer.call(server, {:on_hand, sku})

  @impl true
  def init(now), do: {:ok, %{now: now, stock: %{}, reservations: %{}, next: 1}}

  defp active(state) do
    now = state.now.()
    Map.filter(state.reservations, fn {_, r} -> now < r.expires end)
  end

  defp available_in(state, sku) do
    held = active(state) |> Map.values() |> Enum.filter(&(&1.sku == sku)) |> Enum.map(& &1.qty) |> Enum.sum()
    Map.get(state.stock, sku, 0) - held
  end

  defguardp positive(qty) when is_integer(qty) and qty > 0

  @impl true
  def handle_call({:add, sku, qty}, _from, state) when positive(qty) do
    state = %{state | stock: Map.update(state.stock, sku, qty, &(&1 + qty))}
    {:reply, {:ok, available_in(state, sku)}, state}
  end

  def handle_call({:add, _, _}, _from, state), do: {:reply, {:error, :invalid_quantity}, state}

  def handle_call({:reserve, sku, qty, ttl}, _from, state) do
    cond do
      not Map.has_key?(state.stock, sku) -> {:reply, {:error, :unknown_sku}, state}
      not (is_integer(qty) and qty > 0) -> {:reply, {:error, :invalid_quantity}, state}
      available_in(state, sku) < qty -> {:reply, {:error, :insufficient_stock}, state}
      true ->
        id = state.next
        reservation = %{sku: sku, qty: qty, expires: state.now.() + ttl}
        {:reply, {:ok, id}, %{state | reservations: Map.put(state.reservations, id, reservation), next: id + 1}}
    end
  end

  def handle_call({:confirm, id}, _from, state) do
    case Map.fetch(state.reservations, id) do
      :error ->
        {:reply, {:error, :not_found}, state}

      {:ok, r} ->
        reservations = Map.delete(state.reservations, id)
        if state.now.() >= r.expires do
          {:reply, {:error, :expired}, %{state | reservations: reservations}}
        else
          stock = Map.update!(state.stock, r.sku, &(&1 - r.qty))
          {:reply, :ok, %{state | stock: stock, reservations: reservations}}
        end
    end
  end

  def handle_call({:cancel, id}, _from, state) do
    case Map.fetch(active(state), id) do
      {:ok, _} -> {:reply, :ok, %{state | reservations: Map.delete(state.reservations, id)}}
      :error -> {:reply, {:error, :not_found}, %{state | reservations: Map.delete(state.reservations, id)}}
    end
  end

  def handle_call({:available, sku}, _from, state), do: {:reply, max(available_in(state, sku), 0), state}
  def handle_call({:on_hand, sku}, _from, state), do: {:reply, Map.get(state.stock, sku, 0), state}
end
