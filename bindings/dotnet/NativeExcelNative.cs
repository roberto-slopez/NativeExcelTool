using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

/// <summary>Native C ABI imports and SafeHandle lifetime helpers. Match the process architecture to the Rust DLL.</summary>
public static class NativeExcelNative
{
    private const string Library = "native_excel_ffi";
    [StructLayout(LayoutKind.Sequential)]
    public struct Buffer { public IntPtr Data; public nuint Length; }

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern uint ne_abi_version();
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern void ne_buffer_free(Buffer buffer);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_capabilities_json(out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_schema_json(out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_validate_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_export_start_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_result_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_status_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_next_event_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_cancel(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_release(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_open_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_schema_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_next_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_cancel(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_release(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_input_open_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_input_write_batch_json(ulong id, byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_input_finish(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_input_release(ulong id, out Buffer result);

    public sealed class SafeJobHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        public SafeJobHandle(ulong id) : base(true) => SetHandle((IntPtr)id);
        public ulong Id => (ulong)handle;
        protected override bool ReleaseHandle()
        {
            var status = ne_job_release((ulong)handle, out var buffer);
            ne_buffer_free(buffer);
            return status == 0;
        }
    }

    public sealed class SafeQueryHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        public SafeQueryHandle(ulong id) : base(true) => SetHandle((IntPtr)id);
        public ulong Id => (ulong)handle;
        protected override bool ReleaseHandle()
        {
            var status = ne_query_release((ulong)handle, out var buffer);
            ne_buffer_free(buffer);
            return status == 0;
        }
    }

    public sealed class SafeInputHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        public string InputId { get; }
        public SafeInputHandle(ulong id, string inputId) : base(true)
        {
            SetHandle((IntPtr)id);
            InputId = inputId;
        }
        public ulong Id => (ulong)handle;
        public void WriteBatch(string jsonBatch)
        {
            var bytes = Encoding.UTF8.GetBytes(jsonBatch);
            var status = ne_input_write_batch_json(Id, bytes, (nuint)bytes.Length, out var buffer);
            Read(status, buffer);
        }
        public void Finish()
        {
            var status = ne_input_finish(Id, out var buffer);
            Read(status, buffer);
        }
        protected override bool ReleaseHandle()
        {
            var status = ne_input_release((ulong)handle, out var buffer);
            ne_buffer_free(buffer);
            return status == 0;
        }
    }

    public static JsonElement Read(int status, Buffer buffer)
    {
        try
        {
            var json = Marshal.PtrToStringUTF8(buffer.Data, checked((int)buffer.Length)) ?? "null";
            if (status != 0) throw new InvalidOperationException(json);
            using var document = JsonDocument.Parse(json);
            return document.RootElement.Clone();
        }
        finally
        {
            ne_buffer_free(buffer);
        }
    }

    public static JsonElement Capabilities()
    {
        var status = ne_capabilities_json(out var buffer);
        return Read(status, buffer);
    }

    public static void Validate(string json)
    {
        var bytes = Encoding.UTF8.GetBytes(json);
        var status = ne_validate_json(bytes, (nuint)bytes.Length, out var buffer);
        Read(status, buffer);
    }

    public static SafeInputHandle OpenInput(string optionsJson = "{}")
    {
        var bytes = Encoding.UTF8.GetBytes(optionsJson);
        var status = ne_input_open_json(bytes, (nuint)bytes.Length, out var buffer);
        var info = Read(status, buffer);
        return new SafeInputHandle(info.GetProperty("handle").GetUInt64(), info.GetProperty("inputId").GetString()!);
    }

    public static JsonElement Export(string json)
    {
        var bytes = Encoding.UTF8.GetBytes(json);
        var status = ne_export_start_json(bytes, (nuint)bytes.Length, out var buffer);
        using var job = new SafeJobHandle(Read(status, buffer).GetUInt64());
        status = ne_job_result_json(job.Id, out buffer);
        return Read(status, buffer);
    }

    public static Task<JsonElement> ExportAsync(string json, CancellationToken cancellationToken = default)
    {
        return Task.Run(() =>
        {
            var bytes = Encoding.UTF8.GetBytes(json);
            var status = ne_export_start_json(bytes, (nuint)bytes.Length, out var buffer);
            using var job = new SafeJobHandle(Read(status, buffer).GetUInt64());
            using var reg = cancellationToken.Register(() =>
            {
                ne_job_cancel(job.Id, out var cancelBuf);
                ne_buffer_free(cancelBuf);
            });
            status = ne_job_result_json(job.Id, out buffer);
            return Read(status, buffer);
        }, cancellationToken);
    }

    public static async IAsyncEnumerable<JsonElement> QueryStreamAsync(
        string queryJson,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var bytes = Encoding.UTF8.GetBytes(queryJson);
        var openStatus = ne_query_open_json(bytes, (nuint)bytes.Length, out var openBuf);
        using var query = new SafeQueryHandle(Read(openStatus, openBuf).GetUInt64());
        using var reg = cancellationToken.Register(() =>
        {
            ne_query_cancel(query.Id, out var cancelBuf);
            ne_buffer_free(cancelBuf);
        });
        while (!cancellationToken.IsCancellationRequested)
        {
            var row = await Task.Run(() =>
            {
                var status = ne_query_next_json(query.Id, out var buf);
                return Read(status, buf);
            }, cancellationToken).ConfigureAwait(false);
            if (row.ValueKind == JsonValueKind.Null) yield break;
            yield return row;
        }
    }
}
