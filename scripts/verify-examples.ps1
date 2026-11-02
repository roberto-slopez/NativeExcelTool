param([string]$OutputDirectory = 'output')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$culture = [Globalization.CultureInfo]::InvariantCulture

function Read-Part($archive, [string]$name) {
    $entry = $archive.GetEntry($name)
    if ($null -eq $entry) { throw "Missing XLSX part: $name" }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { return [xml]$reader.ReadToEnd() } finally { $reader.Dispose() }
}

function Read-Orders([string]$path) {
    $archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $path))
    try {
        $sheet = Read-Part $archive 'xl/worksheets/sheet1.xml'
        $table = Read-Part $archive 'xl/tables/table1.xml'
        if ($table.table.name -ne 'SalesOrders' -or $table.table.ref -ne 'A1:H11' -or $table.table.autoFilter.ref -ne 'A1:H11') {
            throw "Expected a filterable SalesOrders table at A1:H11 in $path"
        }
        if (@($sheet.worksheet.sheetData.row).Count -ne 11) { throw "Expected a header and 10 rows in $path" }
        $strings = Read-Part $archive 'xl/sharedStrings.xml'
        $values = @{}
        foreach ($row in $sheet.worksheet.sheetData.row) {
            foreach ($cell in $row.c) {
                $value = [string]$cell.v
                if ($cell.t -eq 's') { $value = [string]$strings.sst.si[[int]$value].InnerText }
                $values[[string]$cell.r] = $value
            }
        }
        if ($values.Count -ne 88) { throw "Expected 88 populated cells in $path" }
        if ($values['A2'] -ne 'ORD-1001' -or $values['A11'] -ne 'ORD-1010' -or $values['B2'] -ne 'Northwind') {
            throw "Order data did not round-trip in $path"
        }
        if ([double]::Parse($values['D2'], $culture) -ne ([datetime]'2026-10-20').ToOADate()) {
            throw "The first date is not an Excel date serial in $path"
        }
        $total = [decimal]0
        foreach ($index in 2..11) {
            $quantity = [decimal]::Parse($values["E$index"], $culture)
            $price = [decimal]::Parse($values["F$index"], $culture)
            $amount = [decimal]::Parse($values["G$index"], $culture)
            if ($amount -ne $quantity * $price) { throw "Incorrect order total in row $index of $path" }
            $total += $amount
            foreach ($column in @('D', 'E', 'F', 'G', 'H')) {
                $cell = @($sheet.worksheet.sheetData.row[$index - 1].c) | Where-Object { $_.r -eq "$column$index" }
                if ($cell.t -eq 's' -or $cell.t -eq 'inlineStr') { throw "Expected typed cells in $path at $column$index" }
                if ($column -eq 'H' -and $cell.t -ne 'b') { throw "Expected a boolean cell at H$index in $path" }
            }
        }
        if ($total -ne [decimal]1831.28 -or $values['H2'] -ne '1' -or $values['H3'] -ne '0') {
            throw "Unexpected totals or payment flags in $path"
        }
        Write-Host "Verified $path : 10 rows, SalesOrders A1:H11, typed cells, total USD 1831.28."
        return $values
    } finally { $archive.Dispose() }
}

$node = Read-Orders (Join-Path $OutputDirectory 'orders-node.xlsx')
$dotnet = Read-Orders (Join-Path $OutputDirectory 'orders-dotnet.xlsx')
foreach ($address in $node.Keys) {
    if ($node[$address] -ne $dotnet[$address]) { throw "Node and C# disagree at cell $address" }
}
Write-Host 'Node.js and C# produced identical table data in all 88 cells.'
