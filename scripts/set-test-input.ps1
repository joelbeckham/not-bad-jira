param([Parameter(Mandatory=$true)][AllowEmptyString()][string]$Value, [int]$TargetProcessId)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$process=if ($TargetProcessId) { Get-Process -Id $TargetProcessId -ErrorAction Stop } else { Get-Process not-bad-jira | Where-Object MainWindowHandle -ne 0 | Sort-Object StartTime | Select-Object -Last 1 }
if (!$process -or $process.ProcessName -ne 'not-bad-jira' -or $process.MainWindowHandle -eq 0) { throw 'No test app window found' }
$root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
$condition=New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ControlTypeProperty),([System.Windows.Automation.ControlType]::Edit)
$inputs=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)
if ($inputs.Count -ne 1){throw 'Expected exactly one test input'}
$pattern=$inputs[0].GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
$pattern.SetValue($Value)
Start-Sleep -Milliseconds 150

