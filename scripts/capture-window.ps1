param([string]$Output = 'design/reviews/native-demo.png', [int]$TargetProcessId)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class CaptureWindow {
 [StructLayout(LayoutKind.Sequential)] public struct RECT {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
}
"@
[CaptureWindow]::SetProcessDPIAware() | Out-Null
$process = if ($TargetProcessId) { Get-Process -Id $TargetProcessId -ErrorAction Stop } else { Get-Process not-bad-jira | Where-Object MainWindowHandle -ne 0 | Sort-Object StartTime | Select-Object -Last 1 }
if (!$process -or $process.ProcessName -ne 'not-bad-jira' -or $process.MainWindowHandle -eq 0) { throw 'No test app window found' }
$rect = New-Object CaptureWindow+RECT
[CaptureWindow]::GetWindowRect($process.MainWindowHandle,[ref]$rect) | Out-Null
$bitmap = New-Object System.Drawing.Bitmap ($rect.Right-$rect.Left),($rect.Bottom-$rect.Top)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$dc=$graphics.GetHdc()
[CaptureWindow]::PrintWindow($process.MainWindowHandle,$dc,2) | Out-Null
$graphics.ReleaseHdc($dc)
$bitmap.Save((Join-Path $PWD $Output),[System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose();$bitmap.Dispose()
Get-Item $Output | Select-Object FullName,Length


