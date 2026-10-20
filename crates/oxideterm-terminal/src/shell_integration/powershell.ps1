if ($ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage') { return }
if ($Host.Name -ne 'ConsoleHost' -or [Console]::IsInputRedirected) { return }
if (Get-Variable __oxideterm_command_hooks -Scope Global -ValueOnly -ErrorAction Ignore) { return }
$readLine = Get-Command PSConsoleHostReadLine -ErrorAction Ignore
if ($readLine -isnot [System.Management.Automation.FunctionInfo]) { return }
$originalPrompt = Get-Command prompt -ErrorAction Ignore
if ($originalPrompt -and $originalPrompt.Definition -match '\]133;|\]633;') { return }

$global:__oxideterm_command_readline = $readLine.ScriptBlock
$global:__oxideterm_command_prompt = if ($originalPrompt) { $originalPrompt.ScriptBlock } else { { 'PS> ' } }
$global:__oxideterm_command_hooks = $true
$global:__oxideterm_command_running = $false

function global:PSConsoleHostReadLine {
    # Preserve the existing line editor, validation, and key bindings. Only a
    # returned command counts as accepted input; cancellation never emits C.
    $line = & $global:__oxideterm_command_readline
    if (-not [string]::IsNullOrWhiteSpace($line)) {
        $global:__oxideterm_command_running = $true
        [Console]::Write("$([char]27)]133;C$([char]7)")
    }
    return $line
}

function global:prompt {
    $succeeded = $?
    $nativeExit = Get-Variable LASTEXITCODE -Scope Global -ValueOnly -ErrorAction Ignore
    $exitCode = 0
    if (-not $succeeded) {
        $lastCommand = Get-History -Count 1
        $powerShellError = $Error.Count -gt 0 -and $lastCommand -and
            $Error[0].InvocationInfo.HistoryId -eq $lastCommand.Id
        $exitCode = if ($powerShellError -or -not $nativeExit) { 1 } else { $nativeExit }
    }
    if ($global:__oxideterm_command_running) {
        [Console]::Write("$([char]27)]133;D;$exitCode$([char]7)")
        $global:__oxideterm_command_running = $false
    }
    [Console]::Write("$([char]27)]133;A$([char]7)")
    # Ignore does not append to $Error or display text. Restore the status
    # before calling prompt themes that inspect $? themselves.
    if (-not $succeeded) { Write-Error 'Command failed.' -ErrorAction Ignore }
    $text = $global:__oxideterm_command_prompt.Invoke()
    return "$text$([char]27)]133;B$([char]7)"
}
