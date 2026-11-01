const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const excel = require('../dist/debug/index.cjs');
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
test('schema is exposed by native library', () => assert.equal(excel.schema().title,'ExportRequest'));
test('typed report total is validated before creating output', () => {
  const request=JSON.parse(fs.readFileSync('examples/configs/typed-report.json','utf8'));
  excel.validate(request);
  request.workbooks[0].worksheets[0].totalRows=3;
  assert.throws(()=>excel.validate(request), /ROW_COUNT_MISMATCH/);
});
test('invalid query options reject before connecting', async () => {
  await assert.rejects(excel.queryStream({dbms:'postgresql',connectionString:'',query:'SELECT 1',options:{queueRows:0}}), /INVALID_CONFIG/);
});
