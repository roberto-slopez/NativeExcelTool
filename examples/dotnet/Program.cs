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
var output = Path.GetFullPath(args.Length > 0 ? args[0] : "output/orders-dotnet.xlsx");
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
    workbook.GetProperty("rows").GetInt32() != 10 || workbook.GetProperty("sheetsCompleted").GetInt32() != 1)
    throw new InvalidOperationException($"Export did not complete: {report}");
Console.WriteLine($"Created {output}: 10 orders in Excel table SalesOrders (A1:H11).");
Console.WriteLine(FormattableString.Invariant($"Total sales: USD {orders.Sum(order => order.Total):0.00}"));

record Order(string OrderId, string Customer, string Product, DateOnly OrderDate, int Quantity, decimal UnitPrice, bool Paid)
{
    public decimal Total => Quantity * UnitPrice;
}
