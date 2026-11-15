param([Parameter(Mandatory=$true)][string]$Name, [int]$TargetProcessId)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$process=if ($TargetProcessId) { Get-Process -Id $TargetProcessId -ErrorAction Stop } else { Get-Process not-bad-jira | Where-Object MainWindowHandle -ne 0 | Sort-Object StartTime | Select-Object -Last 1 }
if (!$process -or $process.ProcessName -ne 'not-bad-jira' -or $process.MainWindowHandle -eq 0) { throw 'No test app window found' }
$root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
$nameCondition=New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty),$Name
$typeCondition=New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ControlTypeProperty),([System.Windows.Automation.ControlType]::Button)
$condition=New-Object System.Windows.Automation.AndCondition $nameCondition,$typeCondition
$button=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
if (!$button){throw "Button not found: $Name"}
$invoke=$button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$invoke.Invoke()
Start-Sleep -Milliseconds 200
