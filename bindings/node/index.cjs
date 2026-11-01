'use strict';
const native = require('./native_excel.node');
const encode = value => JSON.stringify(value, function (_, v) {
  if (typeof v !== 'bigint') return v;
  return this && this.type === 'int64' ? String(v) : { $type: 'int64', value: String(v) };
});
exports.schema = () => JSON.parse(native.schemaJson());
exports.capabilities = () => JSON.parse(native.capabilitiesJson());
exports.validate = request => native.validateJson(encode(request));
exports.exportWorkbooks = async request => JSON.parse(await native.exportJson(encode(request)));
exports.startExport = async request => {
  const job = await native.startExportJson(encode(request));
  return {
    cancel: () => job.cancel(),
    status: () => JSON.parse(job.statusJson()),
    nextEvent: async () => {
      const event = await job.nextEventJson();
      return event == null ? null : JSON.parse(event);
    },
    events() {
      let done = false;
      return {
        async next() {
          if (done) return { done: true };
          const event = await job.nextEventJson();
          if (event == null) { done = true; return { done: true }; }
          return { done: false, value: JSON.parse(event) };
        },
        async return() { done = true; return { done: true }; },
        [Symbol.asyncIterator]() { return this; }
      };
    },
    result: async () => JSON.parse(await job.resultJson())
  };
};
exports.openInputStream = options => {
  const stream = native.openInputJson(options == null ? null : encode(options));
  return {
    inputId: stream.inputId(),
    writeBatch: async records => stream.writeBatchJson(encode(records)),
    finish: () => stream.finish(),
    close: () => stream.close()
  };
};
exports.queryStream = async request => {
  const stream = await native.queryStreamJson(encode(request));
  let finished = false;
  return {
    schema: JSON.parse(stream.schemaJson()),
    async next() {
      if (finished) return { done: true };
      try { const row = await stream.nextJson(); if (row == null) { finished = true; return { done: true }; } return { done: false, value: JSON.parse(row) }; }
      catch (error) { finished = true; await stream.close(); throw error; }
    },
    async return() { finished = true; await stream.close(); return { done: true }; },
    [Symbol.asyncIterator]() { return this; }
  };
};
