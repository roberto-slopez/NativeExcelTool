using System;
using System.IO;
using System.Linq;
using System.Text.Json;

// The project copies the matching native DLL from dist/debug or dist/release.
// These are ordinary C# records; replace them with your application's data.
Order[] orders = [
    new("ORD-1001", "Northwind", "Keyboard", new(2026, 10, 20), 2, 49.95m, true),
    new("ORD-1002", "Contoso", "Monitor", new(2026, 10, 21), 1, 249.50m, false),
    new("ORD-1003", "Fabrikam", "Mouse", new(2026, 10, 22), 5, 24.90m, true),
    new("ORD-1004", "Adventure Works", "USB-C dock", new(2026, 10, 23), 3, 89.00m, true),
    new("ORD-1005", "Tailspin Toys", "Webcam", new(2026, 10, 24), 2, 69.95m, false),
    new("ORD-1006", "Wide World Importers", "Headset", new(2026, 10, 25), 4, 59.50m, true),
    new("ORD-1007", "Woodgrove Bank", "Laptop stand", new(2026, 10, 26), 6, 34.75m, false),
    new("ORD-1008", "Litware", "SSD 1 TB", new(2026, 10, 27), 2, 99.99m, true),
    new("ORD-1009", "Proseware", "HDMI cable", new(2026, 10, 28), 10, 12.50m, true),
    new("ORD-1010", "Coho Winery", "Printer", new(2026, 10, 29), 1, 179.00m, false)
];
var tableOnly = args.Contains("--table-only");
var output = Path.GetFullPath(args.FirstOrDefault(arg => arg != "--table-only") ??
    (tableOnly ? "output/orders-dotnet-table.xlsx" : "output/orders-dotnet.xlsx"));
const string CurrencyFormat = "\"$\"#,##0.00";
var totalSales = orders.Sum(order => order.Total);
var unpaidSales = orders.Where(order => !order.Paid).Sum(order => order.Total);
var reportHeader = new
{
    title = new { text = "Sales Order Report | USD", height = 34, style = new { background = "#16324F", fontColor = "#FFFFFF" } },
    labelHeight = 18,
    valueHeight = 24,
    gapRows = 1,
    labelStyle = new { background = "#E3EDF5", fontColor = "#16324F" },
    valueStyle = new { background = "#F2F6FA", fontColor = "#16324F", fontSize = 13 },
    columns = new[]
    {
        new { fields = new object[]
        {
            new { label = "Period start", value = orders.Min(order => order.OrderDate), dataType = "date", numberFormat = "yyyy-mm-dd" },
            new { label = "Period end", value = orders.Max(order => order.OrderDate), dataType = "date", numberFormat = "yyyy-mm-dd" }
        } },
        new { fields = new object[]
        {
            new { label = "Total sales", value = totalSales, dataType = "number", numberFormat = CurrencyFormat },
            new { label = "Unpaid balance", value = unpaidSales, dataType = "number", numberFormat = CurrencyFormat }
        } },
        new { fields = new object[]
        {
            new { label = "Orders", value = orders.Length, dataType = "integer" },
            new { label = "Paid orders", value = orders.Count(order => order.Paid), dataType = "integer" }
        } }
    }
};
var request = new
{
    schemaVersion = 1,
    styles = new { heading = new { bold = true, background = "#16324F", fontColor = "#FFFFFF" } },
    workbooks = new[] { new
    {
        id = "orders-dotnet",
        output = new { path = output, createParents = true, overwrite = true },
        metadata = new { title = "Sales orders - C# example", author = "NativeExcelTool" },
        worksheets = new[] { new
        {
            name = "Orders", writerMode = "buffered", totalRows = orders.Length,
            table = new { name = "SalesOrders", maxCells = 1000 },
            reportHeader = tableOnly ? null : reportHeader,
            layout = new { headerStyle = "heading", freezeRows = 1 },
            source = new { kind = "objects", records = orders },
            columns = new object[]
            {
                new { field = "orderId", header = "Order", dataType = "text", width = 16 },
                new { field = "customer", header = "Customer", dataType = "text", width = 26 },
                new { field = "product", header = "Product", dataType = "text", width = 20 },
                new { field = "orderDate", header = "Date", dataType = "date", format = "yyyy-mm-dd", width = 16 },
                new { field = "quantity", header = "Quantity", dataType = "integer", width = 12 },
                new { field = "unitPrice", header = "Unit price (USD)", dataType = "number", numberFormat = "\"$\"#,##0.00", width = 20 },
                new { field = "total", header = "Total (USD)", dataType = "number", numberFormat = "\"$\"#,##0.00", width = 18 },
                new { field = "paid", header = "Paid", dataType = "boolean", width = 10 }
            }
        } }
    } }
};
var json = JsonSerializer.Serialize(request, new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase });
NativeExcelNative.Validate(json);
var report = await NativeExcelNative.ExportAsync(json);
var workbook = report.GetProperty("workbooks")[0];
if (workbook.GetProperty("error").ValueKind != JsonValueKind.Null ||
    workbook.GetProperty("rows").GetInt32() != orders.Length || workbook.GetProperty("sheetsCompleted").GetInt32() != 1)
    throw new InvalidOperationException($"Export did not complete: {report}");
Console.WriteLine($"Created {output} with {orders.Length} orders in SalesOrders.");
Console.WriteLine(FormattableString.Invariant($"Total sales: USD {totalSales:0.00}"));

record Order(string OrderId, string Customer, string Product, DateOnly OrderDate, int Quantity, decimal UnitPrice, bool Paid)
{
    public decimal Total => Quantity * UnitPrice;
}
