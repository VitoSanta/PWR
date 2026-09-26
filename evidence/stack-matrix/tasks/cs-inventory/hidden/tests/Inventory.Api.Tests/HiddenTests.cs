using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Mvc.Testing;
using Xunit;

namespace Inventory.Api.Tests;

public class HiddenTests
{
    private static HttpClient NewClient() => new WebApplicationFactory<Program>().CreateClient();

    private static async Task<JsonElement> Json(HttpResponseMessage response) =>
        JsonDocument.Parse(await response.Content.ReadAsStringAsync()).RootElement;

    [Fact]
    public async Task Hidden_search_and_low_stock_combine()
    {
        var client = NewClient();
        await client.PostAsJsonAsync("/items", new { name = "Blue Widget", sku = "B1", quantity = 2, price = 1.0m });
        await client.PostAsJsonAsync("/items", new { name = "Red Widget", sku = "R1", quantity = 50, price = 1.0m });
        await client.PostAsJsonAsync("/items", new { name = "Bolt", sku = "X1", quantity = 1, price = 1.0m });
        var items = await Json(await client.GetAsync("/items?search=WIDGET&lowStock=true"));
        Assert.Equal(1, items.GetArrayLength());
        Assert.Equal("B1", items[0].GetProperty("sku").GetString());
    }

    [Fact]
    public async Task Hidden_adjust_of_unknown_item_is_404()
    {
        var response = await NewClient().PostAsJsonAsync("/items/99/adjust", new { delta = 1 });
        Assert.Equal(HttpStatusCode.NotFound, response.StatusCode);
    }

    [Fact]
    public async Task Hidden_update_to_another_items_sku_is_a_conflict()
    {
        var client = NewClient();
        await client.PostAsJsonAsync("/items", new { name = "A", sku = "S-A", quantity = 1, price = 1.0m });
        await client.PostAsJsonAsync("/items", new { name = "B", sku = "S-B", quantity = 1, price = 1.0m });
        var response = await client.PutAsJsonAsync("/items/2", new { name = "B", sku = "S-A", quantity = 1, price = 1.0m });
        Assert.Equal(HttpStatusCode.Conflict, response.StatusCode);
        var same = await client.PutAsJsonAsync("/items/2", new { name = "B2", sku = "S-B", quantity = 3, price = 2.0m });
        Assert.Equal(HttpStatusCode.OK, same.StatusCode);
    }

    [Fact]
    public async Task Hidden_stats_round_the_value_to_cents()
    {
        var client = NewClient();
        await client.PostAsJsonAsync("/items", new { name = "A", sku = "A", quantity = 3, price = 0.335m });
        var stats = await Json(await client.GetAsync("/stats"));
        Assert.Equal(1, stats.GetProperty("totalItems").GetInt32());
        Assert.Equal(3, stats.GetProperty("totalUnits").GetInt32());
        Assert.Equal(1.00m, stats.GetProperty("inventoryValue").GetDecimal());
    }
}
