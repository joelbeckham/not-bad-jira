param([int[]]$VirtualKeys=@(0x4A,0x0D))
Add-Type @"
using System; using System.Runtime.InteropServices;
public class DirectKeys {
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint msg,IntPtr key,IntPtr data);
 [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code,uint type);
}
"@
$process=Get-Process not-bad-jira | Where-Object MainWindowHandle -ne 0 | Sort-Object StartTime | Select-Object -Last 1
if (!$process){throw 'No app window'}
foreach ($virtualKey in $VirtualKeys) {
 $scan=[DirectKeys]::MapVirtualKey($virtualKey,0)
 $down=[long]1 -bor ([long]$scan -shl 16)
 [DirectKeys]::PostMessage($process.MainWindowHandle,0x100,[IntPtr]$virtualKey,[IntPtr]$down) | Out-Null
 [DirectKeys]::PostMessage($process.MainWindowHandle,0x101,[IntPtr]$virtualKey,[IntPtr]($down -bor 0xC0000000L)) | Out-Null
 Start-Sleep -Milliseconds 100
}
