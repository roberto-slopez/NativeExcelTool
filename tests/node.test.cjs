const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const profile = process.env.NATIVE_EXCEL_PROFILE || 'debug';
if (!['debug', 'release'].includes(profile)) throw new Error('NATIVE_EXCEL_PROFILE must be debug or release');
const excel = require(path.join(__dirname, '../dist', profile, 'index.cjs'));
test('native addon exports two independent workbooks', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'native-excel-node-'));
  try {
    const request = JSON.parse(fs.readFileSync('examples/configs/objects.json','utf8'));
    request.workbooks.forEach((b,i) => b.output.path = path.join(dir,`${i}.xlsx`));
    request.workbooks[0].worksheets[0].source.records[0].id = 9007199254740993n;
    excel.validate(request);
    const report = await excel.exportWorkbooks(request);
    assert.equal(report.workbooks.length,2);
    assert.ok(fs.statSync(path.join(dir,'0.xlsx')).size>1000);
    assert.ok(fs.statSync(path.join(dir,'1.xlsx')).size>1000);
  } finally { fs.rmSync(dir, {recursive:true,force:true}); }
});
test('native validation rejects unknown properties', () => assert.throws(() => excel.validate({workbooks:[],typo:true})));
test('schema and capabilities are exposed by native library', () => {
  assert.equal(excel.schema().title,'ExportRequest');
  const caps = excel.capabilities();
  assert.equal(caps.abiVersion, 1);
  assert.ok(caps.sourceKinds.includes('rowStream'));
  assert.ok(caps.overflowPolicies.includes('splitWorksheet'));
});
test('dynamic report headers are accepted by the native addon', async () => {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'native-excel-header-'));
  try {
    const request=JSON.parse(fs.readFileSync('examples/configs/report-header.json','utf8'));
    request.workbooks[0].output.path=path.join(dir,'report.xlsx');
    excel.validate(request);
    const report=await excel.exportWorkbooks(request);
    assert.equal(report.workbooks[0].rows,2);
    request.workbooks[0].worksheets[0].reportHeader.columnCount=9;
    assert.throws(()=>excel.validate(request),/columnCount/);
  } finally {fs.rmSync(dir,{recursive:true,force:true});}
});
test('native libraries enable file telemetry only with explicit absolute destinations', async () => {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'native-excel-telemetry-'));
  try {
    const request=JSON.parse(fs.readFileSync('examples/configs/typed-report.json','utf8'));
    request.workbooks[0].output.path=path.join(dir,'report.xlsx');
    request.options={telemetry:{logPath:path.join(dir,'events.jsonl'),metricsPath:path.join(dir,'metrics.prom')}};
    await excel.exportWorkbooks(request);
    const events=fs.readFileSync(path.join(dir,'events.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
    assert.equal(events.at(-1).phase,'succeeded');
    assert.ok(fs.readFileSync(path.join(dir,'metrics.prom'),'utf8').includes('native_excel_rows_written{job='));
    request.options.telemetry.logPath='relative.jsonl';
    assert.throws(()=>excel.validate(request),/absolute/);
  } finally { fs.rmSync(dir,{recursive:true,force:true}); }
});
test('typed report total is validated before creating output', () => {
  const request=JSON.parse(fs.readFileSync('examples/configs/typed-report.json','utf8'));
  excel.validate(request);
  request.workbooks[0].worksheets[0].totalRows=3;
  assert.throws(()=>excel.validate(request), /ROW_COUNT_MISMATCH/);
});
test('invalid query options reject before connecting', async () => {
  await assert.rejects(excel.queryStream({dbms:'postgresql',connectionString:'',query:'SELECT 1',options:{queueRows:0}}), /INVALID_CONFIG/);
});
test('incremental caller input stream and worksheet overflow splitting work in Node', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'native-excel-input-'));
  const input = excel.openInputStream({ queueRows: 2 });
  try {
    const outPath = path.join(dir, 'streamed.xlsx');
    const jobPromise = excel.startExport({
      options: { maxRows: 3 },
      workbooks: [{
        id: 'streamed',
        output: { path: outPath },
        worksheets: [{
          name: 'Orders',
          source: { kind: 'rowStream', inputId: input.inputId },
          columns: [
            { field: 'id', dataType: 'integer' },
            { field: 'amount', dataType: 'number' }
          ],
          overflow: { policy: 'splitWorksheet', namePattern: 'Orders_{index}' },
          rules: [{ when: { field: 'amount', op: 'gt', value: 100 }, format: { bold: true } }]
        }]
      }]
    });
    const job = await jobPromise;
    await input.writeBatch([{ id: 1, amount: 50 }, { id: 2, amount: 150 }]);
    await input.writeBatch([{ id: 3, amount: 250 }]);
    input.finish();
    const report = await job.result();
    assert.equal(report.workbooks[0].rows, 3);
    assert.equal(report.workbooks[0].sheetsCompleted, 2);
    assert.ok(fs.statSync(outPath).size > 1000);
  } finally {
    input.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
