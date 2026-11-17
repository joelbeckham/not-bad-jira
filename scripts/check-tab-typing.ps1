param([Parameter(Mandatory=$true)][int]$TargetProcessId)
$ErrorActionPreference='Stop'
$check=Join-Path $PSScriptRoot 'tab-ui-check.ps1'
$keys=Join-Path $PSScriptRoot 'post-test-keys.ps1'
$capture=Join-Path $PSScriptRoot 'capture-window.ps1'
$state=& $check -TargetProcessId $TargetProcessId -Action Dump
if (!($state -match 'Demo workspace')) {throw 'Run this check only in a fresh --demo window'}
$expected='abcdefghijklmnopqrstuvwxyz123456789/'
$typingKeys=@(0x41..0x5A)+@(0x31..0x39)+@(0xBF)
function Open-TabAction([string]$Name) {
 & $check -TargetProcessId $TargetProcessId -Action RightClick -X 70 -Y 60
 Start-Sleep -Milliseconds 150
 & $check -TargetProcessId $TargetProcessId -Action Click -Name $Name
 Start-Sleep -Milliseconds 150
}
function Test-Typing([string]$Field) {
 & $check -TargetProcessId $TargetProcessId -Action Set -Name $Field -Value ''
 & $check -TargetProcessId $TargetProcessId -Action Click -Name $Field
 & $keys -TargetProcessId $TargetProcessId -VirtualKeys $typingKeys
 $actual=& $check -TargetProcessId $TargetProcessId -Action Value -Name $Field
 if ($actual -cne $expected) {throw "$Field swallowed keystrokes: $actual"}
}
Open-TabAction 'Rename tab'
Test-Typing 'Tab name'
& $capture -TargetProcessId $TargetProcessId -Output design/reviews/tab-typing-rename.png
& (Join-Path $PSScriptRoot 'post-test-save-chord.ps1') -TargetProcessId $TargetProcessId
Start-Sleep -Milliseconds 200
Open-TabAction 'Rename tab'
$saved=& $check -TargetProcessId $TargetProcessId -Action Value -Name 'Tab name'
if ($saved -cne $expected) {throw 'Ctrl+Enter did not save the typed name'}
& $keys -TargetProcessId $TargetProcessId -VirtualKeys @(0x1B)
Open-TabAction 'Edit tab'
Test-Typing 'Tab name'
Test-Typing 'JQL'
& $capture -TargetProcessId $TargetProcessId -Output design/reviews/tab-typing-edit.png
& $keys -TargetProcessId $TargetProcessId -VirtualKeys @(0x1B)
Open-TabAction 'Edit tab'
$original=& $check -TargetProcessId $TargetProcessId -Action Value -Name JQL
if ($original -cne 'assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC') {throw 'Escape did not cancel the JQL draft'}
& $keys -TargetProcessId $TargetProcessId -VirtualKeys @(0x1B)
& $check -TargetProcessId $TargetProcessId -Action RightClick -X 650 -Y 60
Start-Sleep -Milliseconds 150
& $check -TargetProcessId $TargetProcessId -Action Click -Name 'New tab'
Test-Typing 'Tab name'
Test-Typing 'JQL'
& $keys -TargetProcessId $TargetProcessId -VirtualKeys @(0x1B)
$state=& $check -TargetProcessId $TargetProcessId -Action Dump
if (!($state -match 'Filter issues')) {throw 'Cancel did not restore the list'}
'PASS: actual alphabet, digit and slash keystrokes in Rename, Edit and New tab; Ctrl+Enter save and Escape cancellation'
