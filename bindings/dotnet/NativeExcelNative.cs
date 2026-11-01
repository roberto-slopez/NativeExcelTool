using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

/// <summary>Native C ABI imports. Match the process architecture to the Rust DLL.</summary>
public static class NativeExcelNative
{
    private const string Library = "native_excel_ffi";
    [StructLayout(LayoutKind.Sequential)]
    public struct Buffer { public IntPtr Data; public nuint Length; }
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern uint ne_abi_version();
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern void ne_buffer_free(Buffer buffer);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_schema_json(out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_validate_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_export_start_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_result_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_status_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_cancel(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_job_release(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_open_json(byte[] data, nuint length, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_schema_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_next_json(ulong id, out Buffer result);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] public static extern int ne_query_release(ulong id, out Buffer result);
    public static JsonElement Read(int status, Buffer buffer)
    {
        try {
            var json = Marshal.PtrToStringUTF8(buffer.Data, checked((int)buffer.Length)) ?? "null";
            if (status != 0) throw new InvalidOperationException(json);
            using var document = JsonDocument.Parse(json);
            return document.RootElement.Clone();
        } finally { ne_buffer_free(buffer); }
    }
    public static JsonElement Export(string json)
    {
        var bytes = Encoding.UTF8.GetBytes(json);
        var status = ne_export_start_json(bytes, (nuint)bytes.Length, out var buffer);
        var id = Read(status, buffer).GetUInt64();
        try { status = ne_job_result_json(id, out buffer); return Read(status, buffer); }
        finally { status = ne_job_release(id, out buffer); Read(status, buffer); }
    }
}
