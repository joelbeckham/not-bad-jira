param(
 [Parameter(Mandatory=$true)][int]$TargetProcessId,
 [int]$Delta=-120,
 [int]$ClientX=400,
 [int]$ClientY=260,
 [switch]$Horizontal
)
Add-Type @"
using System; using System.Runtime.InteropServices;
public class DirectWheel {
 [StructLayout(LayoutKind.Sequential)] public struct POINT {public int X,Y;}
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h,ref POINT p);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint msg,IntPtr w,IntPtr l);
}
"@
$process=Get-Process -Id $TargetProcessId -ErrorAction Stop
if ($process.ProcessName -ne 'not-bad-jira' -or $process.MainWindowHandle -eq 0) {throw 'No test app window found'}
if ($Delta -lt -32768 -or $Delta -gt 32767) {throw 'Wheel delta must fit a signed 16-bit value'}
$point=New-Object DirectWheel+POINT
$point.X=$ClientX;$point.Y=$ClientY
if (![DirectWheel]::ClientToScreen($process.MainWindowHandle,[ref]$point)){throw 'Cannot locate test window'}
$client=([long]$ClientX -band 0xffff) -bor (([long]$ClientY -band 0xffff) -shl 16)
$screen=([long]$point.X -band 0xffff) -bor (([long]$point.Y -band 0xffff) -shl 16)
$wheel=([long]$Delta -band 0xffff) -shl 16
# Send only to the exact test window; never move the system cursor or send global input.
[DirectWheel]::PostMessage($process.MainWindowHandle,0x200,[IntPtr]::Zero,[IntPtr]$client) | Out-Null
$message=if($Horizontal){0x20e}else{0x20a}
[DirectWheel]::PostMessage($process.MainWindowHandle,$message,[IntPtr]$wheel,[IntPtr]$screen) | Out-Null
Start-Sleep -Milliseconds 150
