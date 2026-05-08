fn oxideterm_terminal_env(config: &LocalPtyConfig, _shell: &ShellInfo) -> HashMap<String, String> {
    let mut terminal_env = HashMap::from([
        ("OXIDETERM_TERM".to_string(), "true".to_string()),
        ("TERM".to_string(), "xterm-256color".to_string()),
        ("TERM_PROGRAM".to_string(), "oxideterm".to_string()),
        (
            "TERM_PROGRAM_VERSION".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ),
        ("COLORTERM".to_string(), "truecolor".to_string()),
    ]);

    #[cfg(target_os = "macos")]
    {
        let lang = env::var("LANG").unwrap_or_default();
        if lang.is_empty() || lang == "C" || lang == "POSIX" {
            let detected = std::process::Command::new("defaults")
                .args(["read", ".GlobalPreferences", "AppleLocale"])
                .output()
                .ok()
                .and_then(|output| {
                    output
                        .status
                        .success()
                        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
                })
                .filter(|locale| !locale.is_empty())
                .map(|locale| format!("{locale}.UTF-8"))
                .unwrap_or_else(|| "en_US.UTF-8".to_string());
            terminal_env.insert("LANG".to_string(), detected.clone());
            terminal_env.insert("LC_ALL".to_string(), detected);
        }
    }

    #[cfg(unix)]
    if let Ok(mut path) = env::var("PATH") {
        for additional in ["/usr/local/bin", "/usr/local/sbin", "/opt/homebrew/bin"] {
            if !path.contains(additional) && std::path::Path::new(additional).exists() {
                path.push(':');
                path.push_str(additional);
            }
        }
        terminal_env.insert("PATH".to_string(), path);
    }

    #[cfg(target_os = "windows")]
    {
        terminal_env.insert("PYTHONIOENCODING".to_string(), "utf-8".to_string());
        terminal_env.insert("TERM_PROGRAM".to_string(), "OxideTerm".to_string());
        if _shell.id.starts_with("wsl") {
            terminal_env.insert("WSL_UTF8".to_string(), "1".to_string());
            let mut wslenv_vars = vec!["TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"];
            if config.oh_my_posh_enabled {
                if let Some(theme) = config
                    .oh_my_posh_theme
                    .as_deref()
                    .filter(|theme| !theme.is_empty())
                {
                    terminal_env.insert("POSH_THEME".to_string(), theme.to_string());
                    wslenv_vars.push("POSH_THEME/p");
                }
            }
            terminal_env.insert("WSLENV".to_string(), wslenv_vars.join(":"));
        } else if config.oh_my_posh_enabled {
            if let Some(theme) = config
                .oh_my_posh_theme
                .as_deref()
                .filter(|theme| !theme.is_empty())
            {
                terminal_env.insert("POSH_THEME".to_string(), theme.to_string());
            }
        }
    }

    terminal_env.extend(config.env.clone());
    terminal_env
}

#[cfg(target_os = "windows")]
fn powershell_init_args(config: &LocalPtyConfig, shell: &ShellInfo) -> Option<Vec<String>> {
    if !matches!(shell.id.as_str(), "powershell" | "pwsh") {
        return None;
    }

    let mut init_parts = vec![
        "try { [Console]::InputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}".to_string(),
    ];

    if config.oh_my_posh_enabled {
        let omp = if let Some(theme) = config
            .oh_my_posh_theme
            .as_deref()
            .filter(|theme| !theme.is_empty())
        {
            format!(
                "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {{ oh-my-posh init pwsh --config '{}' | Invoke-Expression }}",
                theme.replace('\'', "''")
            )
        } else {
            "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) { oh-my-posh init pwsh | Invoke-Expression }".to_string()
        };
        init_parts.push(omp);
    }

    init_parts.push("Clear-Host".to_string());
    let cwd = config
        .cwd
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "$HOME".to_string())
        .replace('\'', "''");
    init_parts.push(format!("Set-Location -LiteralPath '{cwd}'"));

    let mut args = shell_args_for_profile(shell, config.load_profile);
    args.push("-Command".to_string());
    args.push(init_parts.join("; "));
    Some(args)
}

fn focus_report_sequence(enabled: bool, focused: bool) -> Option<&'static [u8]> {
    enabled.then_some(if focused { b"\x1b[I" } else { b"\x1b[O" })
}
