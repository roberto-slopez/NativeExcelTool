/** Exact types for the request are also available in export-request.schema.json. */
export type Style = { fontName?: string; fontSize?: number; bold?: boolean; italic?: boolean; underline?: boolean; strikeout?: boolean; fontColor?: string; background?: string; border?: 'thin'|'medium'|'thick'|'double'; borderColor?: string; horizontal?: 'left'|'center'|'right'; vertical?: 'top'|'center'|'bottom'; wrap?: boolean; rotation?: number; numberFormat?: string; locked?: boolean };
export type TaggedValue = { $type: 'int64'|'decimal'|'date'|'timestamp'|'formula'|'json'; value: string };
export type Cell = string|number|bigint|boolean|null|TaggedValue;
export type QueryRequest = { dbms: 'postgresql'|'sqlserver'; connectionString: string; query: string; parameters?: {type:'text'|'int32'|'int64'|'float64'|'boolean'; value: string|number|boolean|null}[]; options?: {connectTimeoutMs?:number;queryTimeoutMs?:number;queueRows?:number;maxRowBytes?:number;allowPlaintext?:boolean;fetchRows?:number} };
export type DataType = 'auto'|'text'|'integer'|'number'|'decimal'|'boolean'|'date'|'datetime'|'time'|'formula';
export type Column = { field: string; dataType?:DataType; numberFormat?:string; header?:string; width?:number; style?:string; format?:Style|string; missingAsNull?:boolean };
export type Worksheet = { name:string; totalRows?:number; writerMode?:'auto'|'buffered'|'streaming'; bodyStyle?:string; table?:{name:string;maxCells?:number}; source:({kind:'objects';records:Record<string,Cell>[]}|({kind:'query'}&QueryRequest)); columns:Column[]; styles?:Record<string,Style>; layout?:{header?:boolean;headerStyle?:string;startRow?:number;startColumn?:number;freezeRows?:number;autofilter?:boolean;rowHeight?:number;merges?:{firstRow:number;firstColumn:number;lastRow:number;lastColumn:number;text:string;style?:string}[]} };
export type ExportRequest = { schemaVersion?:1; styles?:Record<string,Style>; options?:{progressIntervalMs?:number;telemetry?:{logPath?:string;metricsPath?:string;sampleIntervalMs?:number};tempDirectory?:string;maxStyles?:number;maxWorkbooks?:number;maxSheets?:number;maxRows?:number;compression?:6;continueOnError?:boolean};workbooks:{id:string;output:{path:string;overwrite?:boolean;createParents?:boolean};styles?:Record<string,Style>;worksheets:Worksheet[]}[] };
export type ExportReport = {workbooks:{id:string;path:string;rows:number;error:null|{code:string;message:string}}[]};
export type ExportJob = { cancel():void; status():{phase:string;rowsWritten:number;filesPublished:number;totalRows:number|null;telemetryErrors:number};result():Promise<ExportReport> };
export function schema():object;
export function validate(request:ExportRequest):void;
export function exportWorkbooks(request:ExportRequest):Promise<ExportReport>;
export function startExport(request:ExportRequest):Promise<ExportJob>;
export function queryStream(request:QueryRequest):Promise<AsyncIterableIterator<Record<string,Cell>>&{schema:string[]}>;
