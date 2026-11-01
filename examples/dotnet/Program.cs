using System;
using System.IO;
using System.Runtime.InteropServices;
var profile = args.Length > 0 ? args[0] : "debug";
NativeLibrary.SetDllImportResolver(typeof(NativeExcelNative).Assembly, (name, assembly, path) =>
    name == "native_excel_ffi" ? NativeLibrary.Load(Path.GetFullPath($"dist/{profile}/native_excel_ffi.dll")) : IntPtr.Zero);
Console.WriteLine(NativeExcelNative.Export(File.ReadAllText(args.Length > 1 ? args[1] : "examples/configs/objects.json")));
