param(
 [Parameter(Mandatory=$true)][int]$TargetProcessId,
 [ValidateSet('Dump','RightClick','Click','Set','Value','Invoke','Resize')][string]$Action='Dump',
 [string]$Name,
 [AllowEmptyString()][string]$Value,
 [int]$X=0,[int]$Y=0,[int]$Width=1000,[int]$Height=800
)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public class TabCheckMouse {
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint msg,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h,ref POINT p);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int width,int height,uint flags);
 [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
}
"@
$process=Get-Process -Id $TargetProcessId -ErrorAction Stop
if ($process.ProcessName -ne 'not-bad-jira' -or $process.MainWindowHandle -eq 0) {throw 'Expected exact test app process'}
$root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
if($Action -eq 'Resize') {
 [TabCheckMouse]::SetWindowPos($process.MainWindowHandle,[IntPtr]::Zero,0,0,$Width,$Height,6)|Out-Null
 exit
}
if ($Action -eq 'Dump') {
 $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
  $c=$_.Current
  if($c.Name -or $c.ControlType.ProgrammaticName -eq 'ControlType.Edit') { '{0}|{1}|{2}|enabled={3}' -f $c.ControlType.ProgrammaticName,$c.Name,$c.BoundingRectangle,$c.IsEnabled }
 }
 exit
}
if($Name) {
 $condition=New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty),$Name
 $element=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
 if(!$element){throw "Element not found: $Name"}
 if($Action -eq 'Set') {
  $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
  exit
 }
 if($Action -eq 'Value') {
  $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
  exit
 }
 if($Action -eq 'Invoke') {
  $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
  exit
 }
 $bounds=$element.Current.BoundingRectangle
 $point=New-Object TabCheckMouse+POINT
 $point.X=[int]($bounds.Left+$bounds.Width/2); $point.Y=[int]($bounds.Top+$bounds.Height/2)
 [TabCheckMouse]::ScreenToClient($process.MainWindowHandle,[ref]$point)|Out-Null
 $X=$point.X; $Y=$point.Y
}
$coord=[IntPtr](($Y -shl 16) -bor ($X -band 0xffff))
$down=if($Action -eq 'RightClick'){0x204}else{0x201}
$up=if($Action -eq 'RightClick'){0x205}else{0x202}
$button=if($Action -eq 'RightClick'){2}else{1}
[TabCheckMouse]::PostMessage($process.MainWindowHandle,0x200,[IntPtr]0,$coord)|Out-Null
[TabCheckMouse]::PostMessage($process.MainWindowHandle,$down,[IntPtr]$button,$coord)|Out-Null
[TabCheckMouse]::PostMessage($process.MainWindowHandle,$up,[IntPtr]0,$coord)|Out-Null
