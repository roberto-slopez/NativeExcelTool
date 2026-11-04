<#
.SYNOPSIS
Verifies the XLSX files produced by the Node.js and C# sales examples.
.DESCRIPTION
Reads the workbook ZIP/XML parts independently of the export engine. Checks table
boundaries, typed cell values, line totals, and equality between both reports.
.PARAMETER OutputDirectory
Directory containing orders-node.xlsx and orders-dotnet.xlsx. The default is output/.
.EXAMPLE
./scripts/verify-examples.ps1 -OutputDirectory output
#>
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
        if ($table.table.name -ne 'SalesOrders' -or $table.table.ref -ne 'A7:H17' -or $table.table.autoFilter.ref -ne 'A7:H17') {
            throw "Expected a filterable SalesOrders table at A7:H17 in $path"
        }
        $merges = @($sheet.worksheet.mergeCells.mergeCell | ForEach-Object { $_.ref })
        $expectedMerges = @('A1:H1')
        foreach ($index in 2..5) { $expectedMerges += "A${index}:C${index}", "D${index}:F${index}", "G${index}:H${index}" }
        if (Compare-Object $expectedMerges $merges) { throw "Unexpected report header merges in $path" }
        if ($sheet.worksheet.sheetViews.sheetView.pane.topLeftCell -ne 'A8') { throw "Expected report and table headings to be frozen in $path" }
        $strings = Read-Part $archive 'xl/sharedStrings.xml'
        $values = @{}
        foreach ($row in $sheet.worksheet.sheetData.row) {
            foreach ($cell in $row.c) {
                if ($null -eq $cell.v) { continue }
                $value = [string]$cell.v
                if ($cell.t -eq 's') { $value = [string]$strings.sst.si[[int]$value].InnerText }
                $values[[string]$cell.r] = $value
            }
        }
        if ($values.Count -ne 101) { throw "Expected 13 report header cells and 88 table cells in $path" }
        if ($values['A1'] -ne 'Sales Order Report | USD' -or
            $values['A2'] -ne 'Period start' -or $values['A4'] -ne 'Period end' -or
            $values['D2'] -ne 'Total sales' -or $values['D4'] -ne 'Unpaid balance' -or
            $values['G2'] -ne 'Orders' -or $values['G4'] -ne 'Paid orders') {
            throw "Report title or summary labels did not round-trip in $path"
        }
        if ($values['A8'] -ne 'ORD-1001' -or $values['A17'] -ne 'ORD-1010' -or $values['B8'] -ne 'Northwind') {
            throw "Order data did not round-trip in $path"
        }
        if ([double]::Parse($values['A3'], $culture) -ne ([datetime]'2026-10-20').ToOADate() -or
            [double]::Parse($values['A5'], $culture) -ne ([datetime]'2026-10-29').ToOADate() -or
            $values['A3'] -ne $values['D8'] -or $values['A5'] -ne $values['D17']) {
            throw "Report period does not match the Excel dates in $path"
        }
        $total = [decimal]0
        $unpaid = [decimal]0
        $paidCount = 0
        foreach ($index in 8..17) {
            $quantity = [decimal]::Parse($values["E$index"], $culture)
            $price = [decimal]::Parse($values["F$index"], $culture)
            $amount = [decimal]::Parse($values["G$index"], $culture)
            if ($amount -ne $quantity * $price) { throw "Incorrect order total in row $index of $path" }
            $total += $amount
            if ($values["H$index"] -eq '1') { $paidCount++ } else { $unpaid += $amount }
            foreach ($column in @('D', 'E', 'F', 'G', 'H')) {
                $row = $sheet.worksheet.sheetData.row | Where-Object { $_.r -eq "$index" }
                $cell = $row.c | Where-Object { $_.r -eq "$column$index" }
                if ($cell.t -eq 's' -or $cell.t -eq 'inlineStr') { throw "Expected typed cells in $path at $column$index" }
                if ($column -eq 'H' -and $cell.t -ne 'b') { throw "Expected a boolean cell at H$index in $path" }
            }
        }
        if ($total -ne [decimal]1831.28 -or $unpaid -ne [decimal]776.90 -or $paidCount -ne 6 -or
            [decimal]::Parse($values['D3'], $culture) -ne $total -or
            [decimal]::Parse($values['D5'], $culture) -ne $unpaid -or
            $values['G3'] -ne '10' -or [int]$values['G5'] -ne $paidCount) {
            throw "Unexpected totals or payment flags in $path"
        }
        Write-Host "Verified $path with report headers, 10 orders, and USD 1831.28 in sales."
        return $values
    } finally { $archive.Dispose() }
}

$node = Read-Orders (Join-Path $OutputDirectory 'orders-node.xlsx')
$dotnet = Read-Orders (Join-Path $OutputDirectory 'orders-dotnet.xlsx')
foreach ($address in $node.Keys) {
    if ($node[$address] -ne $dotnet[$address]) { throw "Node and C# disagree at cell $address" }
}
Write-Host 'Node.js and C# produced identical report values in all 101 populated cells.'
