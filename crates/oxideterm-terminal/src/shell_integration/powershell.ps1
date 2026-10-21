if ($ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage') { return }
if ($Host.Name -ne 'ConsoleHost' -or [Console]::IsInputRedirected) { return }
$readLine = Get-Command PSConsoleHostReadLine -ErrorAction Ignore
if ($readLine -isnot [System.Management.Automation.FunctionInfo]) { return }

# A profile reload must not capture our own wrapper as its delegate.
if (-not $readLine.Definition.Contains('$global:__oxideterm_command_readline')) {
    $global:__oxideterm_command_readline = $readLine.ScriptBlock
}

function global:PSConsoleHostReadLine {
    # Repair a prompt replaced by a late-loaded theme. The prompt already drawn
    # stays unmarked; never manufacture a command end after that prompt.
    __oxideterm_install_command_prompt -AtReadLine
    # Preserve the existing line editor, validation, and key bindings. Only a
    # returned command counts as accepted input; cancellation never emits C.
    $line = & $global:__oxideterm_command_readline
    if ($global:__oxideterm_command_enabled -and -not [string]::IsNullOrWhiteSpace($line)) {
        $global:__oxideterm_command_running = $true
        [Console]::Write("$([char]27)]133;C;aid=oxideterm$([char]7)")
    }
    return $line
}

$global:__oxideterm_prompt_wrapper = {
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
        [Console]::Write("$([char]27)]133;D;$exitCode;aid=oxideterm$([char]7)")
        $global:__oxideterm_command_running = $false
    }
    [Console]::Write("$([char]27)]133;A;aid=oxideterm$([char]7)")
    # Ignore does not append to $Error or display text. Restore the status
    # before calling prompt themes that inspect $? themselves.
    if (-not $succeeded) { Write-Error 'Command failed.' -ErrorAction Ignore }
    $text = $global:__oxideterm_command_prompt.Invoke()
    return "$text$([char]27)]133;B;aid=oxideterm$([char]7)"
}

function global:__oxideterm_install_command_prompt {
    param([switch]$AtReadLine)
    $current = Get-Command prompt -ErrorAction Ignore
    if ($current -and $current.Definition.Contains('$global:__oxideterm_command_prompt.Invoke()')) {
        $global:__oxideterm_command_enabled = $true
        return
    }
    if ($AtReadLine -and $global:__oxideterm_command_running) {
        # The replacement prompt has already finished drawing. B identifies
        # the new input; the previous output has no verified D boundary.
        [Console]::Write("$([char]27)]133;B;aid=oxideterm$([char]7)")
    }
    $global:__oxideterm_command_running = $false
    if ($current -and $current.Definition -match '\]133;|\]633;') {
        $global:__oxideterm_command_enabled = $false
        return
    }
    $global:__oxideterm_command_prompt = if ($current) { $current.ScriptBlock } else { { 'PS> ' } }
    Set-Item Function:global:prompt -Value $global:__oxideterm_prompt_wrapper
    $global:__oxideterm_command_enabled = $true
}
__oxideterm_install_command_prompt
