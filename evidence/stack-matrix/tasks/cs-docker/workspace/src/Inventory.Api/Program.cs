var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton<Store>();
var app = builder.Build();

app.MapGet("/health", () => Results.Ok(new { status = "ok" }));
app.MapGet("/items", (Store s, string? search, bool? lowStock) =>
{
    IEnumerable<Item> items = s.Items.Values;
    if (!string.IsNullOrEmpty(search)) items = items.Where(i => i.Name.Contains(search, StringComparison.OrdinalIgnoreCase));
    if (lowStock == true) items = items.Where(i => i.Quantity < 5);
    return items.OrderBy(i => i.Name, StringComparer.OrdinalIgnoreCase);
});
app.MapGet("/items/{id:int}", (Store s, int id) => s.Items.TryGetValue(id, out var i) ? Results.Ok(i) : Results.NotFound());
app.MapPost("/items", (Store s, ItemInput input) =>
{
    var errors = Validate(input);
    if (errors.Count > 0) return Results.ValidationProblem(errors);
    if (s.Items.Values.Any(i => i.Sku == input.Sku)) return Results.Conflict();
    var item = new Item(++s.LastId, input.Name!, input.Sku!, input.Quantity, input.Price);
    s.Items[item.Id] = item;
    return Results.Created($"/items/{item.Id}", item);
});
app.MapPut("/items/{id:int}", (Store s, int id, ItemInput input) =>
{
    if (!s.Items.ContainsKey(id)) return Results.NotFound();
    var errors = Validate(input);
    if (errors.Count > 0) return Results.ValidationProblem(errors);
    if (s.Items.Values.Any(i => i.Sku == input.Sku && i.Id != id)) return Results.Conflict();
    var item = new Item(id, input.Name!, input.Sku!, input.Quantity, input.Price);
    s.Items[id] = item;
    return Results.Ok(item);
});
app.MapPost("/items/{id:int}/adjust", (Store s, int id, Adjust adjust) =>
{
    if (!s.Items.TryGetValue(id, out var item)) return Results.NotFound();
    if (item.Quantity + adjust.Delta < 0) return Results.Conflict(new { error = "insufficient stock" });
    item = item with { Quantity = item.Quantity + adjust.Delta };
    s.Items[id] = item;
    return Results.Ok(item);
});
app.MapDelete("/items/{id:int}", (Store s, int id) => s.Items.Remove(id) ? Results.NoContent() : Results.NotFound());
app.MapGet("/stats", (Store s) => new
{
    totalItems = s.Items.Count,
    totalUnits = s.Items.Values.Sum(i => i.Quantity),
    inventoryValue = Math.Round(s.Items.Values.Sum(i => i.Quantity * i.Price), 2),
});
app.Run();

static Dictionary<string, string[]> Validate(ItemInput input)
{
    var errors = new Dictionary<string, string[]>();
    if (string.IsNullOrWhiteSpace(input.Name)) errors["name"] = ["Name is required."];
    if (string.IsNullOrWhiteSpace(input.Sku)) errors["sku"] = ["SKU is required."];
    if (input.Quantity < 0) errors["quantity"] = ["Quantity cannot be negative."];
    if (input.Price < 0) errors["price"] = ["Price cannot be negative."];
    return errors;
}

public record Item(int Id, string Name, string Sku, int Quantity, decimal Price);
public record ItemInput(string? Name, string? Sku, int Quantity, decimal Price);
public record Adjust(int Delta);
public class Store { public Dictionary<int, Item> Items { get; } = new(); public int LastId { get; set; } }
public partial class Program { }
