/**
 * Public request and response types for the Node.js wrapper.
 * The generated export-request.schema.json describes the serialized JSON contract.
 * The wrapper additionally accepts bigint values and encodes them as tagged integers.
 */
export type BorderStyle = 'thin'|'medium'|'thick'|'double'|'dashed'|'dotted';
export type FillPattern = 'solid'|'none'|'gray125'|'gray0625'|'lightGray'|'mediumGray'|'darkGray';
export type Style = { fontName?: string; fontSize?: number; bold?: boolean; italic?: boolean; underline?: boolean; strikeout?: boolean; fontColor?: string; background?: string; foregroundColor?: string; fillPattern?: FillPattern; border?: BorderStyle; borderColor?: string; borderLeft?: BorderStyle; borderLeftColor?: string; borderRight?: BorderStyle; borderRightColor?: string; borderTop?: BorderStyle; borderTopColor?: string; borderBottom?: BorderStyle; borderBottomColor?: string; borderDiagonal?: BorderStyle; borderDiagonalColor?: string; horizontal?: 'left'|'center'|'right'; vertical?: 'top'|'center'|'bottom'; wrap?: boolean; shrinkToFit?: boolean; indent?: number; rotation?: number; numberFormat?: string; locked?: boolean; hidden?: boolean; resetFields?: string[] };
export type TaggedValue = { $type: 'int64'|'decimal'|'date'|'timestamp'|'formula'|'json'|'binary'; value: string; encoding?: 'hex'|'base64' };
export type Cell = string|number|bigint|boolean|null|TaggedValue|Record<string,unknown>|number[];
export type QueryRequest = { dbms: 'postgresql'|'sqlserver'; connectionString: string; query: string; parameters?: {type:'text'|'int32'|'int64'|'float64'|'boolean'; value: string|number|boolean|null}[]; options?: {connectTimeoutMs?:number;queryTimeoutMs?:number;queueRows?:number;maxRowBytes?:number;allowPlaintext?:boolean;fetchRows?:number} };
export type DataType = 'auto'|'text'|'integer'|'number'|'decimal'|'boolean'|'date'|'datetime'|'time'|'formula'|'binary';
export type ColumnTransform = 'trim'|'uppercase'|'lowercase';
export type PrecisionPolicy = 'exactText'|'numeric'|'round';
export type BinaryEncoding = 'hex'|'base64';
export type Column = { field: string; path?: string[]; dataType?:DataType; numberFormat?:string; header?:string; width?:number; style?:string; format?:Style|string; missingAsNull?:boolean; defaultValue?:Cell; nullPlaceholder?:string; transforms?:ColumnTransform[]; precisionPolicy?:PrecisionPolicy; roundScale?:number; timezone?:string; binaryEncoding?:BinaryEncoding };
export type HeaderField = { label:string; value:Cell; dataType?:DataType; numberFormat?:string; labelStyle?:Style; valueStyle?:Style };
export type ReportHeader = { title?:{text:string;style?:Style;height?:number};columnCount?:number;columns?:{fields:HeaderField[]}[];labelStyle?:Style;valueStyle?:Style;labelHeight?:number;valueHeight?:number;gapRows?:number };
export type RuleCondition = { rowParity?:'odd'|'even'; field?:string; op?:'eq'|'ne'|'gt'|'gte'|'lt'|'lte'|'isNull'|'notNull'; value?:Cell; all?:RuleCondition[]; any?:RuleCondition[]; not?:RuleCondition };
export type StyleRule = { when:RuleCondition; field?:string; style?:string; format?:Style };
export type CellStyleOverride = { row:number; field:string; style?:string; format?:Style };
export type OverflowSpec = { policy?:'error'|'splitWorksheet'|'splitWorkbook'; namePattern?:string; maxSplits?:number };
export type PrintOptions = { landscape?:boolean; paperSize?:number; fitToWidth?:number; fitToHeight?:number; repeatRows?:[number,number]; header?:string; footer?:string; margins?:{left:number;right:number;top:number;bottom:number;header:number;footer:number} };
export type Worksheet = { name:string; inferColumns?:boolean; caseInsensitiveFields?:boolean; extraFields?:'ignore'|'error'; reportHeader?:ReportHeader; totalRows?:number; writerMode?:'auto'|'buffered'|'streaming'; bodyStyle?:string; table?:{name:string;maxCells?:number}; overflow?:OverflowSpec; rules?:StyleRule[]; cellOverrides?:CellStyleOverride[]; source:({kind:'objects';records:Record<string,Cell>[]}|({kind:'query'}&QueryRequest)|{kind:'rowStream';inputId:string}); columns?:Column[]; styles?:Record<string,Style>; layout?:{header?:boolean;headerStyle?:string;startRow?:number;startColumn?:number;freezeRows?:number;freezeColumns?:number;autofilter?:boolean;autofit?:boolean;autofitSampleRows?:number;rowHeight?:number;rowHeights?:Record<string,number>;hiddenColumns?:number[];hiddenRows?:number[];gridlines?:boolean;zoom?:number;tabColor?:string;protectSheet?:boolean;print?:PrintOptions;merges?:{firstRow:number;firstColumn:number;lastRow:number;lastColumn:number;text:string;style?:string}[]} };
export type ExportDefaults = { writerMode?:'auto'|'buffered'|'streaming'; bodyStyle?:string; headerStyle?:string; rowHeight?:number; autofilter?:boolean; freezeRows?:number };
export type WorkbookMetadata = { title?:string; author?:string; subject?:string; company?:string; manager?:string; category?:string; keywords?:string; comments?:string };
export type ExportRequest = { schemaVersion?:1; styles?:Record<string,Style>; defaults?:ExportDefaults; options?:{progressIntervalMs?:number;exportTimeoutMs?:number;telemetry?:{logPath?:string;metricsPath?:string;sampleIntervalMs?:number};tempDirectory?:string;maxStyles?:number;maxWorkbooks?:number;maxSheets?:number;maxRows?:number;maxOutputBytes?:number;compression?:6;continueOnError?:boolean};workbooks:{id:string;output:{path:string;overwrite?:boolean;createParents?:boolean;returnBytes?:boolean;syncData?:boolean};metadata?:WorkbookMetadata;defaults?:ExportDefaults;styles?:Record<string,Style>;worksheets:Worksheet[]}[] };
export type ExportReport = {workbooks:{id:string;path:string;rows:number;sheetsCompleted:number;splitFiles?:string[];bytesBase64?:string;error:null|{code:string;message:string}}[]};
export type JobStatus = {phase:string;rowsWritten:number;filesPublished:number;totalRows:number|null;telemetryErrors:number};
export type ExportJob = { cancel():void; status():JobStatus; nextEvent():Promise<JobStatus|null>; events():AsyncIterableIterator<JobStatus>; result():Promise<ExportReport> };
export type InputOptions = { queueRows?:number; maxBatchBytes?:number; maxRowBytes?:number };
export type InputStreamHandle = { inputId:string; writeBatch(records:Record<string,Cell>[]):Promise<void>; finish():void; close():void };
export type Capabilities = { abiVersion:number; schemaVersion:number; dbms:string[]; writerModes:string[]; sourceKinds:string[]; overflowPolicies:string[]; streamingTables:boolean; maxRows:number; maxColumns:number };
export function schema():object;
export function capabilities():Capabilities;
export function validate(request:ExportRequest):void;
export function exportWorkbooks(request:ExportRequest):Promise<ExportReport>;
export function startExport(request:ExportRequest):Promise<ExportJob>;
export function openInputStream(options?:InputOptions):InputStreamHandle;
export function queryStream(request:QueryRequest):Promise<AsyncIterableIterator<Record<string,Cell>>&{schema:string[]}>;
