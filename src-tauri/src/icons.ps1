# Turns exe / lnk paths and Store apps into 64px PNG icons (data URLs).
# In:  JSON [{ "id": "...", "src": "C:\\path.exe" | "pkg:PackageName" }]
# Out: JSON { "id": "data:image/png;base64,..." }
param([string]$In, [string]$Out)
$ErrorActionPreference = 'SilentlyContinue'
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System; using System.IO; using System.Runtime.InteropServices; using System.Drawing; using System.Drawing.Imaging;
public static class H3Icon {
  [ComImport, Guid("bcc18b79-ba16-442f-80c4-8a59c30c463b"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IShellItemImageFactory { [PreserveSig] int GetImage(SIZE size, int flags, out IntPtr phbm); }
  [StructLayout(LayoutKind.Sequential)] struct SIZE { public int cx, cy; }
  [StructLayout(LayoutKind.Sequential)] struct BITMAP { public int bmType, bmWidth, bmHeight, bmWidthBytes; public ushort bmPlanes, bmBitsPixel; public IntPtr bmBits; }
  [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
  static extern int SHCreateItemFromParsingName(string path, IntPtr pbc, ref Guid riid, [MarshalAs(UnmanagedType.Interface)] out IShellItemImageFactory item);
  [DllImport("gdi32.dll")] static extern int GetObject(IntPtr h, int c, ref BITMAP b);
  [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr h);
  public static string Png(string path, int size) {
    IShellItemImageFactory f; Guid g = typeof(IShellItemImageFactory).GUID;
    if (SHCreateItemFromParsingName(path, IntPtr.Zero, ref g, out f) != 0 || f == null) return null;
    IntPtr hb; SIZE s; s.cx = size; s.cy = size;
    if (f.GetImage(s, 0x4 /* icon only */, out hb) != 0) return null;
    try {
      BITMAP bm = new BITMAP(); GetObject(hb, Marshal.SizeOf(bm), ref bm);
      if (bm.bmBits == IntPtr.Zero || bm.bmBitsPixel != 32) return null;
      int h = Math.Abs(bm.bmHeight);
      using (var src = new Bitmap(bm.bmWidth, h, bm.bmWidthBytes, PixelFormat.Format32bppArgb, bm.bmBits))
      using (var copy = new Bitmap(src)) {
        if (bm.bmHeight > 0) copy.RotateFlip(RotateFlipType.RotateNoneFlipY);   // bottom-up DIB
        using (var ms = new MemoryStream()) { copy.Save(ms, ImageFormat.Png); return "data:image/png;base64," + Convert.ToBase64String(ms.ToArray()); }
      }
    } finally { DeleteObject(hb); }
  }
}
'@
$items = Get-Content -LiteralPath $In -Raw -Encoding UTF8 | ConvertFrom-Json
$res = @{}
foreach ($i in $items) {
    $src = [Environment]::ExpandEnvironmentVariables($i.src)
    if ($src.StartsWith('pkg:')) {
        $p = Get-AppxPackage -Name $src.Substring(4) | Select-Object -First 1
        if (-not $p) { continue }
        $app = (Get-AppxPackageManifest $p).Package.Applications.Application | Select-Object -First 1
        $src = "shell:AppsFolder\$($p.PackageFamilyName)!$($app.Id)"
    } elseif (-not (Test-Path -LiteralPath $src)) { continue }
    $png = [H3Icon]::Png($src, 64)
    if ($png) { $res[$i.id] = $png }
}
$res | ConvertTo-Json -Compress | Set-Content -LiteralPath $Out -Encoding UTF8
