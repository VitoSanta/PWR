# Inventory API

An HTTP API for a small warehouse, in ASP.NET Core (.NET 10, minimal APIs).
The acceptance tests in `tests/Inventory.Api.Tests` are the contract; they start
the application in memory through `WebApplicationFactory<Program>`.

## Layout expected by the tests

- `src/Inventory.Api/Inventory.Api.csproj`: a `Microsoft.NET.Sdk.Web` project
  targeting `net10.0`, whose `Program` is reachable from the tests (for example
  with `public partial class Program { }` at the end of `Program.cs`).
- `Inventory.slnx` (or `.sln`) at the root containing both projects is welcome
  but not required.

## Resource

An item: `id` (int, assigned by the server: 1, 2, 3... never reused), `name`,
`sku`, `quantity` (int), `price` (decimal). JSON uses camelCase.

Storage is in memory and belongs to the running application: every test
starts a new application and must see an empty store.

## Endpoints

| Method and path | Result |
|---|---|
| `GET /health` | `200 {"status":"ok"}` |
| `POST /items` | `201`, `Location: /items/{id}`, body the created item |
| `GET /items` | `200`, all items sorted by name ignoring case; `?search=` keeps names containing the text (ignoring case); `?lowStock=true` keeps items with quantity below 5 |
| `GET /items/{id}` | `200` item, or `404` |
| `PUT /items/{id}` | replaces the item: `200` with the item, `404` if unknown |
| `POST /items/{id}/adjust` | body `{"delta": n}`: adds `n` to the quantity, `200` with the item; `409` if the result would be negative (quantity unchanged); `404` if unknown |
| `DELETE /items/{id}` | `204`, or `404` |
| `GET /stats` | `200 {"totalItems", "totalUnits", "inventoryValue"}`, value = sum of quantity x price, rounded to 2 decimals |

## Validation

On `POST` and `PUT`: `name` and `sku` required, `quantity >= 0`, `price >= 0`.
A violation answers `400` as a validation problem (`application/problem+json`)
whose `errors` object has one key per invalid field, named in camelCase
(`name`, `sku`, `quantity`, `price`). A `sku` already used by another item
answers `409`.

Run the tests with `dotnet test tests/Inventory.Api.Tests`.
