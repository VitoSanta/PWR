using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Mvc.Testing;
using Xunit;

namespace Inventory.Api.Tests;

/// Acceptance tests for the Inventory API. Each test starts its own
/// application, so the store must belong to the application (not be static).
public class InventoryApiTests
{
    private static HttpClient NewClient() => new WebApplicationFactory<Program>().CreateClient();

    private static async Task<JsonElement> Json(HttpResponseMessage response) =>
        JsonDocument.Parse(await response.Content.ReadAsStringAsync()).RootElement;

    private static Task<HttpResponseMessage> Create(HttpClient client, string name, string sku, int quantity, decimal price) =>
        client.PostAsJsonAsync("/items", new { name, sku, quantity, price });

    [Fact]
    public async Task Health_is_ok()
    {
        var response = await NewClient().GetAsync("/health");
        Assert.Equal(HttpStatusCode.OK, response.StatusCode);
        Assert.Equal("ok", (await Json(response)).GetProperty("status").GetString());
    }

    [Fact]
    public async Task Create_returns_201_with_location_and_the_item()
    {
        var client = NewClient();
        var response = await Create(client, "Widget", "W-1", 10, 2.50m);
        Assert.Equal(HttpStatusCode.Created, response.StatusCode);
        Assert.Equal("/items/1", response.Headers.Location?.ToString());
        var item = await Json(response);
        Assert.Equal(1, item.GetProperty("id").GetInt32());
        Assert.Equal("Widget", item.GetProperty("name").GetString());
        Assert.Equal("W-1", item.GetProperty("sku").GetString());
        Assert.Equal(10, item.GetProperty("quantity").GetInt32());
        Assert.Equal(2.50m, item.GetProperty("price").GetDecimal());
    }

    [Fact]
    public async Task Create_rejects_invalid_fields_with_a_validation_problem()
    {
        var response = await Create(NewClient(), "", "X-1", -1, -2m);
        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode);
        Assert.Equal("application/problem+json", response.Content.Headers.ContentType?.MediaType);
        var errors = (await Json(response)).GetProperty("errors");
        Assert.True(errors.TryGetProperty("name", out _), "errors.name");
        Assert.True(errors.TryGetProperty("quantity", out _), "errors.quantity");
        Assert.True(errors.TryGetProperty("price", out _), "errors.price");
    }

    [Fact]
    public async Task Duplicate_sku_is_a_conflict()
    {
        var client = NewClient();
        await Create(client, "Widget", "W-1", 1, 1m);
        var response = await Create(client, "Other", "W-1", 1, 1m);
        Assert.Equal(HttpStatusCode.Conflict, response.StatusCode);
    }

    [Fact]
    public async Task Unknown_item_is_404()
    {
        var response = await NewClient().GetAsync("/items/99");
        Assert.Equal(HttpStatusCode.NotFound, response.StatusCode);
    }

    [Fact]
    public async Task List_is_sorted_by_name_ignoring_case_and_searchable()
    {
        var client = NewClient();
        await Create(client, "Widget", "W", 1, 1m);
        await Create(client, "apple", "A", 1, 1m);
        await Create(client, "Gadget", "G", 1, 1m);
        var all = await Json(await client.GetAsync("/items"));
        Assert.Equal(new[] { "apple", "Gadget", "Widget" }, all.EnumerateArray().Select(i => i.GetProperty("name").GetString()!).ToArray());
        var found = await Json(await client.GetAsync("/items?search=GAD"));
        Assert.Equal(new[] { "Gadget" }, found.EnumerateArray().Select(i => i.GetProperty("name").GetString()!).ToArray());
    }

    [Fact]
    public async Task Low_stock_filter_returns_items_below_five()
    {
        var client = NewClient();
        await Create(client, "A", "A", 2, 1m);
        await Create(client, "B", "B", 10, 1m);
        await Create(client, "C", "C", 4, 1m);
        await Create(client, "D", "D", 5, 1m);
        var low = await Json(await client.GetAsync("/items?lowStock=true"));
        Assert.Equal(new[] { "A", "C" }, low.EnumerateArray().Select(i => i.GetProperty("name").GetString()!).ToArray());
    }

    [Fact]
    public async Task Update_replaces_the_item()
    {
        var client = NewClient();
        await Create(client, "Widget", "W-1", 10, 2m);
        var response = await client.PutAsJsonAsync("/items/1", new { name = "Widget XL", sku = "W-1", quantity = 3, price = 4.5m });
        Assert.Equal(HttpStatusCode.OK, response.StatusCode);
        var item = await Json(await client.GetAsync("/items/1"));
        Assert.Equal("Widget XL", item.GetProperty("name").GetString());
        Assert.Equal(3, item.GetProperty("quantity").GetInt32());
    }

    [Fact]
    public async Task Update_of_unknown_item_is_404_and_invalid_update_is_400()
    {
        var client = NewClient();
        var missing = await client.PutAsJsonAsync("/items/5", new { name = "X", sku = "X", quantity = 1, price = 1m });
        Assert.Equal(HttpStatusCode.NotFound, missing.StatusCode);
        await Create(client, "Widget", "W-1", 10, 2m);
        var invalid = await client.PutAsJsonAsync("/items/1", new { name = "Widget", sku = "W-1", quantity = -5, price = 1m });
        Assert.Equal(HttpStatusCode.BadRequest, invalid.StatusCode);
    }

    [Fact]
    public async Task Adjust_changes_stock_and_refuses_to_go_negative()
    {
        var client = NewClient();
        await Create(client, "Widget", "W-1", 10, 2m);
        var ok = await client.PostAsJsonAsync("/items/1/adjust", new { delta = -3 });
        Assert.Equal(HttpStatusCode.OK, ok.StatusCode);
        Assert.Equal(7, (await Json(ok)).GetProperty("quantity").GetInt32());
        var tooMuch = await client.PostAsJsonAsync("/items/1/adjust", new { delta = -20 });
        Assert.Equal(HttpStatusCode.Conflict, tooMuch.StatusCode);
        var item = await Json(await client.GetAsync("/items/1"));
        Assert.Equal(7, item.GetProperty("quantity").GetInt32());
    }

    [Fact]
    public async Task Delete_removes_and_ids_are_not_reused()
    {
        var client = NewClient();
        await Create(client, "A", "A", 1, 1m);
        Assert.Equal(HttpStatusCode.NoContent, (await client.DeleteAsync("/items/1")).StatusCode);
        Assert.Equal(HttpStatusCode.NotFound, (await client.DeleteAsync("/items/1")).StatusCode);
        var next = await Json(await Create(client, "B", "B", 1, 1m));
        Assert.Equal(2, next.GetProperty("id").GetInt32());
    }

    [Fact]
    public async Task Stats_sum_units_and_value()
    {
        var client = NewClient();
        await Create(client, "A", "A", 2, 1.50m);
        await Create(client, "B", "B", 3, 2.25m);
        var stats = await Json(await client.GetAsync("/stats"));
        Assert.Equal(2, stats.GetProperty("totalItems").GetInt32());
        Assert.Equal(5, stats.GetProperty("totalUnits").GetInt32());
        Assert.Equal(9.75m, stats.GetProperty("inventoryValue").GetDecimal());
    }
}
