using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

/// <summary>
/// Provides C ABI imports and managed lifetime helpers for the native Excel engine.
/// </summary>
/// <remarks>
/// The process architecture must match the native DLL. Raw imports return zero on
/// success and an owned JSON buffer on success or failure. Pass each buffer to
/// <see cref="Read"/> or release it exactly once with <see cref="ne_buffer_free"/>.
/// Raw query reads, event polling, input writes, and job result calls can block.
/// </remarks>
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

    /// <summary>Releases a native job handle and requests cancellation when disposed.</summary>
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

    /// <summary>Releases a native query handle and requests producer cancellation when disposed.</summary>
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

    /// <summary>Owns an incremental input handle and releases it when disposed.</summary>
    /// <remarks>Disposing an unfinished input stream requests cancellation of its consumer.</remarks>
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

    /// <summary>Decodes a native JSON response and releases its buffer in all cases.</summary>
    /// <param name="status">The status returned by the native operation.</param>
    /// <param name="buffer">An unreleased buffer returned by this native library.</param>
    /// <returns>A JSON value that remains valid after the native buffer is released.</returns>
    /// <exception cref="InvalidOperationException">The native operation returned an error.</exception>
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

    /// <summary>Returns the export features supported by the loaded native library.</summary>
    public static JsonElement Capabilities()
    {
        var status = ne_capabilities_json(out var buffer);
        return Read(status, buffer);
    }

    /// <summary>Validates an export request without starting an export.</summary>
    /// <param name="json">The serialized export request.</param>
    public static void Validate(string json)
    {
        var bytes = Encoding.UTF8.GetBytes(json);
        var status = ne_validate_json(bytes, (nuint)bytes.Length, out var buffer);
        Read(status, buffer);
    }

    /// <summary>Creates a bounded input stream for caller-provided records.</summary>
    /// <param name="optionsJson">Serialized input limits, or an empty JSON object for defaults.</param>
    /// <returns>An input handle that the caller must dispose.</returns>
    public static SafeInputHandle OpenInput(string optionsJson = "{}")
    {
        var bytes = Encoding.UTF8.GetBytes(optionsJson);
        var status = ne_input_open_json(bytes, (nuint)bytes.Length, out var buffer);
        var info = Read(status, buffer);
        return new SafeInputHandle(info.GetProperty("handle").GetUInt64(), info.GetProperty("inputId").GetString()!);
    }

    /// <summary>Exports workbooks and blocks until the native job completes.</summary>
    /// <param name="json">The serialized export request.</param>
    /// <returns>The completed export report.</returns>
    public static JsonElement Export(string json)
    {
        var bytes = Encoding.UTF8.GetBytes(json);
        var status = ne_export_start_json(bytes, (nuint)bytes.Length, out var buffer);
        using var job = new SafeJobHandle(Read(status, buffer).GetUInt64());
        status = ne_job_result_json(job.Id, out buffer);
        return Read(status, buffer);
    }

    /// <summary>Waits for a native export on a managed worker thread.</summary>
    /// <param name="json">The serialized export request.</param>
    /// <param name="cancellationToken">Requests cooperative cancellation of the native job.</param>
    /// <returns>A task containing the completed native export report.</returns>
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

    /// <summary>Reads native query rows asynchronously and releases the query on disposal.</summary>
    /// <param name="queryJson">The serialized query configuration.</param>
    /// <param name="cancellationToken">Requests cancellation of the native query producer.</param>
    /// <returns>A sequence of decoded query rows.</returns>
    /// <remarks>Opening the native query occurs synchronously before the first row is awaited.</remarks>
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
