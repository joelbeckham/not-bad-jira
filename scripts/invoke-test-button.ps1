param([Parameter(Mandatory=$true)][string]$Name)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$process=Get-Process not-bad-jira | Where-Object MainWindowHandle -ne 0 | Sort-Object StartTime | Select-Object -Last 1
if (!$process){throw 'No app window'}
$root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
$condition=New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty),$Name
$button=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
if (!$button){throw "Button not found: $Name"}
$invoke=$button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$invoke.Invoke()
Start-Sleep -Milliseconds 200
