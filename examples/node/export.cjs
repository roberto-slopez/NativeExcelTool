'use strict';
const path = require('node:path');

// Run from the repository or extracted alpha bundle. No database is required.
const profile = process.env.NATIVE_EXCEL_PROFILE || 'debug';
if (!['debug', 'release'].includes(profile)) throw new Error('NATIVE_EXCEL_PROFILE must be debug or release');
const excel = require(path.join(__dirname, '../../dist', profile, 'index.cjs'));
const args = process.argv.slice(2);
const tableOnly = args.includes('--table-only');
const output = path.resolve(args.find(arg => arg !== '--table-only')
  || (tableOnly ? 'output/orders-node-table.xlsx' : 'output/orders-node.xlsx'));

const orders = [
  { orderId: 'ORD-1001', customer: 'Northwind', product: 'Keyboard', orderDate: '2026-10-20', quantity: 2, unitPrice: 49.95, paid: true },
  { orderId: 'ORD-1002', customer: 'Contoso', product: 'Monitor', orderDate: '2026-10-21', quantity: 1, unitPrice: 249.50, paid: false },
  { orderId: 'ORD-1003', customer: 'Fabrikam', product: 'Mouse', orderDate: '2026-10-22', quantity: 5, unitPrice: 24.90, paid: true },
  { orderId: 'ORD-1004', customer: 'Adventure Works', product: 'USB-C dock', orderDate: '2026-10-23', quantity: 3, unitPrice: 89.00, paid: true },
  { orderId: 'ORD-1005', customer: 'Tailspin Toys', product: 'Webcam', orderDate: '2026-10-24', quantity: 2, unitPrice: 69.95, paid: false },
  { orderId: 'ORD-1006', customer: 'Wide World Importers', product: 'Headset', orderDate: '2026-10-25', quantity: 4, unitPrice: 59.50, paid: true },
  { orderId: 'ORD-1007', customer: 'Woodgrove Bank', product: 'Laptop stand', orderDate: '2026-10-26', quantity: 6, unitPrice: 34.75, paid: false },
  { orderId: 'ORD-1008', customer: 'Litware', product: 'SSD 1 TB', orderDate: '2026-10-27', quantity: 2, unitPrice: 99.99, paid: true },
  { orderId: 'ORD-1009', customer: 'Proseware', product: 'HDMI cable', orderDate: '2026-10-28', quantity: 10, unitPrice: 12.50, paid: true },
  { orderId: 'ORD-1010', customer: 'Coho Winery', product: 'Printer', orderDate: '2026-10-29', quantity: 1, unitPrice: 179.00, paid: false }
].map(order => ({ ...order, total: Math.round(order.quantity * order.unitPrice * 100) / 100 }));

const currencyFormat = '"$"#,##0.00';
const dates = orders.map(order => order.orderDate).sort();
const totalSales = orders.reduce((sum, order) => sum + Math.round(order.total * 100), 0) / 100;
const unpaidSales = orders.reduce((sum, order) => sum + (order.paid ? 0 : Math.round(order.total * 100)), 0) / 100;
const reportHeader = {
  title: { text: 'Sales Order Report | USD', height: 34, style: { background: '#16324F', fontColor: '#FFFFFF' } },
  labelHeight: 18,
  valueHeight: 24,
  gapRows: 1,
  labelStyle: { background: '#E3EDF5', fontColor: '#16324F' },
  valueStyle: { background: '#F2F6FA', fontColor: '#16324F', fontSize: 13 },
  columns: [
    { fields: [
      { label: 'Period start', value: dates[0], dataType: 'date', numberFormat: 'yyyy-mm-dd' },
      { label: 'Period end', value: dates[dates.length - 1], dataType: 'date', numberFormat: 'yyyy-mm-dd' }
    ] },
    { fields: [
      { label: 'Total sales', value: totalSales, dataType: 'number', numberFormat: currencyFormat },
      { label: 'Unpaid balance', value: unpaidSales, dataType: 'number', numberFormat: currencyFormat }
    ] },
    { fields: [
      { label: 'Orders', value: orders.length, dataType: 'integer' },
      { label: 'Paid orders', value: orders.filter(order => order.paid).length, dataType: 'integer' }
    ] }
  ]
};

const request = {
  schemaVersion: 1,
  styles: { heading: { bold: true, background: '#16324F', fontColor: '#FFFFFF' } },
  workbooks: [{
    id: 'orders-node',
    output: { path: output, createParents: true, overwrite: true },
    metadata: { title: 'Sales orders - Node.js example', author: 'NativeExcelTool' },
    worksheets: [{
      name: 'Orders', writerMode: 'buffered', totalRows: orders.length,
      table: { name: 'SalesOrders', maxCells: 1000 },
      ...(tableOnly ? {} : { reportHeader }),
      layout: { headerStyle: 'heading', freezeRows: 1 },
      source: { kind: 'objects', records: orders },
      columns: [
        { field: 'orderId', header: 'Order', dataType: 'text', width: 16 },
        { field: 'customer', header: 'Customer', dataType: 'text', width: 26 },
        { field: 'product', header: 'Product', dataType: 'text', width: 20 },
        { field: 'orderDate', header: 'Date', dataType: 'date', format: 'yyyy-mm-dd', width: 16 },
        { field: 'quantity', header: 'Quantity', dataType: 'integer', width: 12 },
        { field: 'unitPrice', header: 'Unit price (USD)', dataType: 'number', numberFormat: '"$"#,##0.00', width: 20 },
        { field: 'total', header: 'Total (USD)', dataType: 'number', numberFormat: '"$"#,##0.00', width: 18 },
        { field: 'paid', header: 'Paid', dataType: 'boolean', width: 10 }
      ]
    }]
  }]
};

async function main() {
  excel.validate(request);
  const report = await excel.exportWorkbooks(request);
  const workbook = report.workbooks[0];
  if (workbook.error || workbook.rows !== orders.length || workbook.sheetsCompleted !== 1) {
    throw new Error(`Export did not complete: ${JSON.stringify(report)}`);
  }
  console.log(`Created ${output} with ${orders.length} orders in SalesOrders.`);
  console.log(`Total sales: USD ${totalSales.toFixed(2)}`);
}

main().catch(error => { console.error(error); process.exitCode = 1; });
