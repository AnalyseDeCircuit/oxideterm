// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Runs the POSIX Redis scripts on Windows hosts through a Git/MSYS `bash`.
//!
//! Redis key browsing and value reads are POSIX shell scripts; requiring a
//! second PowerShell dialect would duplicate every parser and every write path.
//! The script travels as base64 so quotes and `$()` survive the PowerShell
//! command line, and stdin (password/db/key payloads) is inherited by the
//! `bash -c` child.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const BASH_CANDIDATES: &str = r#"'bash','C:\Program Files\Git\bin\bash.exe','C:\Program Files\Git\usr\bin\bash.exe','C:\Program Files (x86)\Git\bin\bash.exe','C:\msys64\usr\bin\bash.exe'"#;

pub(crate) fn posix_via_bash(script: &str, section: &str, missing: &str) -> String {
    let encoded = STANDARD.encode(script.as_bytes());
    // PowerShell 5.1 does not escape embedded double quotes when it forwards an
    // argument to a native exe, so `bash -c $script` mangles the script. Write
    // it to a short-lived temp file instead; stdin stays free for payloads.
    let bridge = format!(
        "$oxideScript=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')); \
         $oxideBash=$null; \
         foreach($oxideCandidate in @({BASH_CANDIDATES})){{ \
         if(Test-Path -LiteralPath $oxideCandidate -PathType Leaf){{ $oxideBash=$oxideCandidate; break }}; \
         $oxideCmd=Get-Command $oxideCandidate -ErrorAction SilentlyContinue; \
         if($oxideCmd){{ $oxideBash=$oxideCmd.Source; break }} }}; \
         if($oxideBash){{ \
         $oxidePath=[IO.Path]::Combine([IO.Path]::GetTempPath(), 'oxide-'+$PID+'-'+[Guid]::NewGuid().ToString('N')+'.sh'); \
         try {{ \
         [IO.File]::WriteAllText($oxidePath, $oxideScript, (New-Object System.Text.UTF8Encoding($false))); \
         & $oxideBash $oxidePath.Replace('\\','/') \
         }} finally {{ Remove-Item -LiteralPath $oxidePath -Force -ErrorAction SilentlyContinue }} \
         }} else {{ Write-Output '==={section}==='; Write-Output ('UNAVAILABLE'+[char]9+'{missing}'); Write-Output '==={section}_END===' }}"
    );
    // Local captures and SSH exec both start the line in cmd.exe, so the
    // PowerShell text must be an explicit invocation. The bridge only uses
    // single quotes, which keeps `-Command "…"` unambiguous.
    debug_assert!(!bridge.contains('"'), "bridge must stay single-quoted");
    format!("powershell -NoProfile -NonInteractive -Command \"{bridge}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_scripts_decode_back_to_the_posix_source() {
        let script = "echo '===REDISVALUE==='; IFS= read -r oxide_pw; echo '===REDISVALUE_END==='";
        let command = posix_via_bash(script, "REDISVALUE", "Git Bash not found");
        assert!(command.contains("& $oxideBash $oxidePath.Replace"));
        let start = command.find("FromBase64String('").unwrap() + "FromBase64String('".len();
        let end = command[start..].find('\'').unwrap() + start;
        assert_eq!(
            String::from_utf8(STANDARD.decode(&command[start..end]).unwrap()).unwrap(),
            script
        );
        assert!(command.contains("Write-Output '===REDISVALUE==='"));
    }
}
