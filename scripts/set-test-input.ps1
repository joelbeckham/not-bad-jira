param([Parameter(Mandatory=$true)][AllowEmptyString()][string]$Value)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$process=Get-Process not-bad-jira | Where-Object MainWindowHandle -ne 0 | Sort-Object StartTime | Select-Object -Last 1
$root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
$condition=New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ControlTypeProperty),([System.Windows.Automation.ControlType]::Edit)
$inputs=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)
if ($inputs.Count -ne 1){throw 'Expected exactly one test input'}
$pattern=$inputs[0].GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
$pattern.SetValue($Value)
Start-Sleep -Milliseconds 150

