const fs = require('node:fs');
const excel = require('../../dist/debug/index.cjs');
const request = JSON.parse(fs.readFileSync(process.argv[2] || 'examples/configs/objects.json','utf8'));
excel.exportWorkbooks(request).then(console.log).catch(error => { console.error(error); process.exitCode = 1; });
