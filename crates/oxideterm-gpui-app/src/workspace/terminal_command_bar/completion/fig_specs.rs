pub(in crate::workspace) const TERMINAL_COMMAND_SPECS_FILENAME: &str =
    "terminal-command-specs.json";

#[derive(Clone)]
struct TerminalFigUserSpecCache {
    path: std::path::PathBuf,
    modified: Option<std::time::SystemTime>,
    specs: Vec<TerminalFigSpec>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TerminalFigSpecConfigRoot {
    Specs { specs: Vec<TerminalFigSpecConfig> },
    List(Vec<TerminalFigSpecConfig>),
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalFigSpecConfig {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    subcommands: Vec<TerminalFigSubcommandConfig>,
    #[serde(default)]
    options: Vec<TerminalFigOptionConfig>,
    #[serde(default)]
    args: TerminalFigArgType,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TerminalFigSubcommandConfig {
    Name(String),
    Spec {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        options: Vec<TerminalFigOptionConfig>,
        #[serde(default)]
        args: TerminalFigArgType,
    },
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalFigOptionConfig {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    args: TerminalFigArgType,
}

impl From<TerminalFigOptionConfig> for TerminalFigOptionSpec {
    fn from(config: TerminalFigOptionConfig) -> Self {
        Self {
            name: config.name,
            description: config.description,
            args: config.args,
        }
    }
}

impl TerminalFigOptionConfig {
    fn into_value(self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "name": self.name,
            "args": terminal_fig_arg_type_value(self.args),
        });
        if let Some(description) = self.description {
            value["description"] = serde_json::json!(description);
        }
        value
    }
}

impl From<TerminalFigSubcommandConfig> for TerminalFigSubcommandSpec {
    fn from(config: TerminalFigSubcommandConfig) -> Self {
        match config {
            TerminalFigSubcommandConfig::Name(name) => subcommand(name),
            TerminalFigSubcommandConfig::Spec {
                name,
                description,
                options,
                args,
            } => TerminalFigSubcommandSpec {
                name,
                description,
                options: options.into_iter().map(Into::into).collect(),
                args,
            },
        }
    }
}

impl TerminalFigSubcommandConfig {
    fn into_value(self) -> serde_json::Value {
        match self {
            Self::Name(name) => serde_json::json!(name),
            Self::Spec {
                name,
                description,
                options,
                args,
            } => {
                let mut value = serde_json::json!({
                    "name": name,
                    "options": options.into_iter().map(TerminalFigOptionConfig::into_value).collect::<Vec<_>>(),
                    "args": terminal_fig_arg_type_value(args),
                });
                if let Some(description) = description {
                    value["description"] = serde_json::json!(description);
                }
                value
            }
        }
    }
}

impl From<TerminalFigSpecConfig> for TerminalFigSpec {
    fn from(config: TerminalFigSpecConfig) -> Self {
        TerminalFigSpec {
            description: config.description.unwrap_or_else(|| config.name.clone()),
            name: config.name,
            subcommands: config.subcommands.into_iter().map(Into::into).collect(),
            options: config.options.into_iter().map(Into::into).collect(),
            args: config.args,
        }
    }
}

impl TerminalFigSpecConfig {
    fn into_value(self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "name": self.name,
            "subcommands": self.subcommands.into_iter().map(TerminalFigSubcommandConfig::into_value).collect::<Vec<_>>(),
            "options": self.options.into_iter().map(TerminalFigOptionConfig::into_value).collect::<Vec<_>>(),
            "args": terminal_fig_arg_type_value(self.args),
        });
        if let Some(description) = self.description {
            value["description"] = serde_json::json!(description);
        }
        value
    }
}

fn terminal_fig_arg_type_value(arg_type: TerminalFigArgType) -> &'static str {
    match arg_type {
        TerminalFigArgType::None => "none",
        TerminalFigArgType::Path => "path",
        TerminalFigArgType::File => "file",
        TerminalFigArgType::Directory => "directory",
        TerminalFigArgType::Value => "value",
        TerminalFigArgType::Command => "command",
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_fig_specs(&self) -> Vec<TerminalFigSpec> {
        let mut specs = built_in_terminal_fig_specs();
        for custom in load_user_terminal_fig_specs(self.settings_store.path()) {
            if let Some(existing) = specs.iter_mut().find(|spec| spec.name == custom.name) {
                *existing = custom;
            } else {
                specs.push(custom);
            }
        }
        specs
    }
}

pub(in crate::workspace) fn terminal_command_specs_path(
    settings_path: &std::path::Path,
) -> std::path::PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(TERMINAL_COMMAND_SPECS_FILENAME)
}

pub(in crate::workspace) fn terminal_command_specs_example_json() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "specs": [
            {
                "name": "demo",
                "description": "Demo command",
                "subcommands": [
                    {
                        "name": "run",
                        "description": "Run demo",
                        "options": [
                            { "name": "--profile", "description": "Profile name", "args": "value" }
                        ],
                        "args": "path"
                    }
                ],
                "options": [
                    { "name": "--config", "description": "Config file", "args": "file" }
                ],
                "args": "path"
            }
        ]
    }))
    .unwrap_or_else(|_| "{\n  \"specs\": []\n}".to_string())
}

pub(in crate::workspace) fn normalize_terminal_command_specs_json(
    value: &str,
) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok("{\n  \"specs\": []\n}".to_string());
    }
    let root = serde_json::from_str::<TerminalFigSpecConfigRoot>(trimmed)
        .map_err(|error| error.to_string())?;
    let specs = match root {
        TerminalFigSpecConfigRoot::Specs { specs } => specs,
        TerminalFigSpecConfigRoot::List(specs) => specs,
    };
    let mut normalized = Vec::new();
    for spec in specs {
        if spec.name.trim().is_empty() {
            return Err("Command spec name is required.".to_string());
        }
        normalized.push(spec.into_value());
    }
    serde_json::to_string_pretty(&serde_json::json!({ "specs": normalized }))
        .map_err(|error| error.to_string())
}

pub(in crate::workspace) fn user_terminal_fig_specs_count(
    settings_path: &std::path::Path,
) -> usize {
    load_user_terminal_fig_specs(settings_path).len()
}

fn load_user_terminal_fig_specs(settings_path: &std::path::Path) -> Vec<TerminalFigSpec> {
    static USER_TERMINAL_FIG_SPECS: std::sync::OnceLock<
        std::sync::Mutex<Option<TerminalFigUserSpecCache>>,
    > = std::sync::OnceLock::new();
    let path = terminal_command_specs_path(settings_path);

    let modified = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok();
    let cache = USER_TERMINAL_FIG_SPECS.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some(snapshot) = guard.as_ref()
        && snapshot.path == path
        && snapshot.modified == modified
    {
        return snapshot.specs.clone();
    }

    let Ok(bytes) = std::fs::read(&path) else {
        if let Ok(mut guard) = cache.lock() {
            *guard = Some(TerminalFigUserSpecCache {
                path,
                modified,
                specs: Vec::new(),
            });
        }
        return Vec::new();
    };
    let Ok(root) = serde_json::from_slice::<TerminalFigSpecConfigRoot>(&bytes) else {
        if let Ok(mut guard) = cache.lock() {
            *guard = Some(TerminalFigUserSpecCache {
                path,
                modified,
                specs: Vec::new(),
            });
        }
        return Vec::new();
    };
    let configs = match root {
        TerminalFigSpecConfigRoot::Specs { specs } => specs,
        TerminalFigSpecConfigRoot::List(specs) => specs,
    };
    let specs = configs
        .into_iter()
        .filter(|spec| !spec.name.trim().is_empty())
        .map(Into::into)
        .collect::<Vec<_>>();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(TerminalFigUserSpecCache {
            path,
            modified,
            specs: specs.clone(),
        });
    }
    specs
}

fn option(
    name: impl Into<String>,
    description: Option<&str>,
    args: TerminalFigArgType,
) -> TerminalFigOptionSpec {
    TerminalFigOptionSpec {
        name: name.into(),
        description: description.map(ToString::to_string),
        args,
    }
}

fn subcommand(name: impl Into<String>) -> TerminalFigSubcommandSpec {
    TerminalFigSubcommandSpec {
        name: name.into(),
        description: None,
        options: Vec::new(),
        args: TerminalFigArgType::None,
    }
}

fn subcommand_with_options(
    name: impl Into<String>,
    description: Option<&str>,
    options: Vec<TerminalFigOptionSpec>,
    args: TerminalFigArgType,
) -> TerminalFigSubcommandSpec {
    TerminalFigSubcommandSpec {
        name: name.into(),
        description: description.map(ToString::to_string),
        options,
        args,
    }
}

fn subcommands(names: &[&str]) -> Vec<TerminalFigSubcommandSpec> {
    names.iter().copied().map(subcommand).collect()
}

fn common_terminal_fig_options() -> Vec<TerminalFigOptionSpec> {
    use TerminalFigArgType::None as NoArg;
    vec![
        option("-h", Some("Show help"), NoArg),
        option("--help", Some("Show help"), NoArg),
        option("-v", Some("Verbose output"), NoArg),
        option("--version", Some("Show version"), NoArg),
    ]
}

fn terminal_fig_spec(
    name: &str,
    description: &str,
    subcommands: Vec<TerminalFigSubcommandSpec>,
    extra_options: Vec<TerminalFigOptionSpec>,
    args: TerminalFigArgType,
) -> TerminalFigSpec {
    let mut options = common_terminal_fig_options();
    options.extend(extra_options);
    TerminalFigSpec {
        name: name.to_string(),
        description: description.to_string(),
        subcommands,
        options,
        args,
    }
}

pub(in crate::workspace) fn built_in_terminal_fig_specs() -> Vec<TerminalFigSpec> {
    use TerminalFigArgType::{Command, Directory, File, None as NoArg, Path, Value};
    vec![
        terminal_fig_spec(
            "git",
            "Distributed version control",
            vec![
                subcommand_with_options(
                    "add",
                    Some("Add file contents to the index"),
                    vec![option("-p", None, NoArg), option("--patch", None, NoArg)],
                    Path,
                ),
                subcommand("branch"),
                subcommand("checkout"),
                subcommand_with_options(
                    "clone",
                    Some("Clone a repository"),
                    vec![option("--depth", None, Value), option("-b", None, Value)],
                    Path,
                ),
                subcommand_with_options(
                    "commit",
                    Some("Record changes"),
                    vec![
                        option("-m", Some("Commit message"), Value),
                        option("--amend", None, NoArg),
                        option("--no-edit", None, NoArg),
                    ],
                    NoArg,
                ),
                subcommand_with_options(
                    "diff",
                    Some("Show changes"),
                    vec![
                        option("--cached", None, NoArg),
                        option("--stat", None, NoArg),
                    ],
                    Path,
                ),
                subcommand("fetch"),
                subcommand("init"),
                subcommand_with_options(
                    "log",
                    Some("Show commit logs"),
                    vec![
                        option("--oneline", None, NoArg),
                        option("--graph", None, NoArg),
                    ],
                    NoArg,
                ),
                subcommand("merge"),
                subcommand("pull"),
                subcommand("push"),
                subcommand_with_options(
                    "rebase",
                    Some("Reapply commits"),
                    vec![
                        option("-i", None, NoArg),
                        option("--continue", None, NoArg),
                        option("--abort", None, NoArg),
                    ],
                    NoArg,
                ),
                subcommand("remote"),
                subcommand("reset"),
                subcommand("restore"),
                subcommand("show"),
                subcommand("stash"),
                subcommand("status"),
                subcommand("switch"),
            ],
            vec![
                option("-C", Some("Run as if git was started in path"), Directory),
                option("--git-dir", None, Directory),
                option("--work-tree", None, Directory),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "npm",
            "Node package manager",
            subcommands(&[
                "install", "run", "test", "start", "build", "publish", "update", "init", "exec",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "pnpm",
            "Fast Node package manager",
            subcommands(&[
                "install", "run", "test", "start", "build", "add", "remove", "update", "exec",
                "dlx",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "yarn",
            "Node package manager",
            subcommands(&[
                "install", "run", "test", "start", "build", "add", "remove", "upgrade", "dlx",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "bun",
            "JavaScript runtime and toolkit",
            subcommands(&["run", "test", "install", "add", "remove", "build", "x"]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "node",
            "JavaScript runtime",
            subcommands(&[]),
            vec![option("-e", None, Value)],
            File,
        ),
        terminal_fig_spec(
            "python",
            "Python interpreter",
            subcommands(&["-m"]),
            vec![option("-m", None, Command)],
            File,
        ),
        terminal_fig_spec(
            "pip",
            "Python package installer",
            subcommands(&["install", "uninstall", "list", "show", "freeze", "search"]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "cargo",
            "Rust package manager",
            vec![
                subcommand_with_options(
                    "build",
                    None,
                    vec![
                        option("--release", None, NoArg),
                        option("--target", None, Value),
                    ],
                    NoArg,
                ),
                subcommand("check"),
                subcommand("clippy"),
                subcommand("doc"),
                subcommand("fmt"),
                subcommand("new"),
                subcommand_with_options(
                    "run",
                    None,
                    vec![
                        option("--release", None, NoArg),
                        option("--bin", None, Value),
                    ],
                    NoArg,
                ),
                subcommand_with_options(
                    "test",
                    None,
                    vec![
                        option("--release", None, NoArg),
                        option("--package", None, Value),
                    ],
                    NoArg,
                ),
                subcommand("update"),
            ],
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "rustup",
            "Rust toolchain manager",
            subcommands(&[
                "default",
                "show",
                "target",
                "toolchain",
                "update",
                "component",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "docker",
            "Container platform",
            subcommands(&[
                "build", "compose", "exec", "images", "logs", "ps", "pull", "push", "rm", "rmi",
                "run", "start", "stop",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "kubectl",
            "Kubernetes CLI",
            subcommands(&[
                "apply",
                "config",
                "create",
                "delete",
                "describe",
                "exec",
                "get",
                "logs",
                "patch",
                "port-forward",
            ]),
            vec![
                option("-f", Some("Filename, directory, or URL"), Path),
                option("--filename", None, Path),
                option("-n", Some("Namespace"), Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "ssh",
            "OpenSSH remote login",
            subcommands(&[]),
            vec![
                option("-i", None, File),
                option("-p", None, Value),
                option("-J", Some("Jump host"), Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "scp",
            "Secure copy",
            subcommands(&[]),
            vec![option("-i", None, File), option("-P", None, Value)],
            Path,
        ),
        terminal_fig_spec(
            "rsync",
            "Fast file copy",
            subcommands(&[]),
            vec![
                option("-a", None, NoArg),
                option("-z", None, NoArg),
                option("--delete", None, NoArg),
            ],
            Path,
        ),
        terminal_fig_spec(
            "tar",
            "Archive utility",
            subcommands(&[]),
            vec![option("-f", None, File), option("-C", None, Directory)],
            Path,
        ),
        terminal_fig_spec(
            "curl",
            "Transfer URLs",
            subcommands(&[]),
            vec![
                option("-o", None, File),
                option("-H", None, Value),
                option("-X", None, Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "wget",
            "Download files",
            subcommands(&[]),
            vec![option("-O", None, File), option("-P", None, Directory)],
            NoArg,
        ),
        terminal_fig_spec(
            "grep",
            "Search text",
            subcommands(&[]),
            vec![
                option("-r", None, NoArg),
                option("-i", None, NoArg),
                option("-n", None, NoArg),
            ],
            Path,
        ),
        terminal_fig_spec(
            "rg",
            "ripgrep search",
            subcommands(&[]),
            vec![
                option("-i", None, NoArg),
                option("-n", None, NoArg),
                option("--glob", None, Value),
            ],
            Path,
        ),
        terminal_fig_spec(
            "find",
            "Find files",
            subcommands(&[]),
            vec![option("-name", None, Value), option("-type", None, Value)],
            Directory,
        ),
        terminal_fig_spec(
            "ls",
            "List directory contents",
            subcommands(&[]),
            vec![
                option("-a", None, NoArg),
                option("-l", None, NoArg),
                option("-s", None, NoArg),
                option("--all", None, NoArg),
            ],
            Path,
        ),
        terminal_fig_spec(
            "cd",
            "Change directory",
            subcommands(&[]),
            vec![],
            Directory,
        ),
        terminal_fig_spec(
            "mkdir",
            "Create directories",
            subcommands(&[]),
            vec![option("-p", None, NoArg)],
            Directory,
        ),
        terminal_fig_spec(
            "rm",
            "Remove files",
            subcommands(&[]),
            vec![option("-r", None, NoArg), option("-f", None, NoArg)],
            Path,
        ),
        terminal_fig_spec(
            "cp",
            "Copy files",
            subcommands(&[]),
            vec![option("-r", None, NoArg), option("-p", None, NoArg)],
            Path,
        ),
        terminal_fig_spec("mv", "Move files", subcommands(&[]), vec![], Path),
        terminal_fig_spec(
            "chmod",
            "Change file modes",
            subcommands(&[]),
            vec![option("-R", None, NoArg)],
            Path,
        ),
        terminal_fig_spec(
            "chown",
            "Change owner/group",
            subcommands(&[]),
            vec![option("-R", None, NoArg)],
            Path,
        ),
        terminal_fig_spec(
            "ps",
            "Process status",
            subcommands(&[]),
            vec![
                option("-a", None, NoArg),
                option("-u", None, NoArg),
                option("-x", None, NoArg),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "kill",
            "Send signal to process",
            subcommands(&[]),
            vec![option("-9", None, NoArg), option("-TERM", None, NoArg)],
            NoArg,
        ),
        terminal_fig_spec(
            "systemctl",
            "Control systemd",
            subcommands(&[
                "status",
                "start",
                "stop",
                "restart",
                "reload",
                "enable",
                "disable",
                "list-units",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "brew",
            "Homebrew package manager",
            subcommands(&[
                "install",
                "uninstall",
                "update",
                "upgrade",
                "search",
                "info",
                "services",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "gh",
            "GitHub CLI",
            subcommands(&["auth", "browse", "issue", "pr", "repo", "run", "workflow"]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "sed",
            "Stream editor",
            subcommands(&[]),
            vec![
                option("-E", None, NoArg),
                option("-e", None, Value),
                option("-f", None, File),
                option("-i", None, Value),
            ],
            Path,
        ),
        terminal_fig_spec(
            "awk",
            "Pattern scanning and processing language",
            subcommands(&[]),
            vec![
                option("-F", Some("Input field separator"), Value),
                option("-f", None, File),
                option("-v", Some("Variable assignment"), Value),
            ],
            Path,
        ),
        terminal_fig_spec(
            "jq",
            "JSON processor",
            subcommands(&[]),
            vec![
                option("-r", None, NoArg),
                option("-c", None, NoArg),
                option("-f", None, File),
                option("--arg", None, Value),
            ],
            File,
        ),
        terminal_fig_spec(
            "make",
            "Build automation",
            subcommands(&[]),
            vec![
                option("-C", None, Directory),
                option("-f", None, File),
                option("-j", None, Value),
            ],
            Value,
        ),
        terminal_fig_spec(
            "cmake",
            "Cross-platform build system",
            subcommands(&["--build", "--install", "--preset", "--workflow"]),
            vec![
                option("-S", None, Directory),
                option("-B", None, Directory),
                option("-D", None, Value),
                option("-G", None, Value),
            ],
            Directory,
        ),
        terminal_fig_spec(
            "go",
            "Go toolchain",
            subcommands(&[
                "build", "clean", "doc", "env", "fmt", "generate", "get", "install", "list", "mod",
                "run", "test", "version",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "rustc",
            "Rust compiler",
            subcommands(&[]),
            vec![
                option("--crate-name", None, Value),
                option("--crate-type", None, Value),
                option("--edition", None, Value),
                option("-o", None, File),
            ],
            File,
        ),
        terminal_fig_spec(
            "uv",
            "Python package and project manager",
            subcommands(&[
                "add", "build", "init", "lock", "pip", "python", "remove", "run", "sync", "tool",
                "venv",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "poetry",
            "Python packaging and dependency manager",
            subcommands(&[
                "add", "build", "check", "config", "env", "export", "install", "lock", "publish",
                "remove", "run", "shell", "update",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "psql",
            "PostgreSQL interactive terminal",
            subcommands(&[]),
            vec![
                option("-d", Some("Database"), Value),
                option("-h", Some("Host"), Value),
                option("-p", Some("Port"), Value),
                option("-U", Some("User"), Value),
                option("-f", None, File),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "mysql",
            "MySQL client",
            subcommands(&[]),
            vec![
                option("-h", Some("Host"), Value),
                option("-P", Some("Port"), Value),
                option("-u", Some("User"), Value),
                option("-D", Some("Database"), Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "redis-cli",
            "Redis command line interface",
            subcommands(&[]),
            vec![
                option("-h", Some("Host"), Value),
                option("-p", Some("Port"), Value),
                option("-n", Some("Database"), Value),
                option("--tls", None, NoArg),
            ],
            Value,
        ),
        terminal_fig_spec(
            "tmux",
            "Terminal multiplexer",
            subcommands(&[
                "attach",
                "detach",
                "kill-session",
                "list-sessions",
                "new-session",
                "split-window",
                "switch-client",
            ]),
            vec![option("-L", None, Value), option("-S", None, File)],
            NoArg,
        ),
        terminal_fig_spec(
            "ssh-keygen",
            "OpenSSH authentication key utility",
            subcommands(&[]),
            vec![
                option("-t", Some("Key type"), Value),
                option("-f", Some("Output file"), File),
                option("-C", Some("Comment"), Value),
                option("-N", Some("New passphrase"), Value),
                option("-y", None, NoArg),
            ],
            File,
        ),
        terminal_fig_spec(
            "openssl",
            "OpenSSL command line tool",
            subcommands(&[
                "asn1parse",
                "dgst",
                "enc",
                "genpkey",
                "pkcs12",
                "rand",
                "req",
                "rsa",
                "s_client",
                "x509",
            ]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "journalctl",
            "Query systemd journal",
            subcommands(&[]),
            vec![
                option("-u", Some("Unit"), Value),
                option("-f", Some("Follow"), NoArg),
                option("-n", Some("Lines"), Value),
                option("--since", None, Value),
                option("--until", None, Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "du",
            "Estimate file space usage",
            subcommands(&[]),
            vec![
                option("-h", None, NoArg),
                option("-s", None, NoArg),
                option("-d", None, Value),
            ],
            Path,
        ),
        terminal_fig_spec(
            "df",
            "Report file system disk space",
            subcommands(&[]),
            vec![
                option("-h", None, NoArg),
                option("-i", None, NoArg),
                option("-T", None, NoArg),
            ],
            Path,
        ),
        terminal_fig_spec(
            "netstat",
            "Network statistics",
            subcommands(&[]),
            vec![
                option("-a", None, NoArg),
                option("-n", None, NoArg),
                option("-p", None, NoArg),
                option("-t", None, NoArg),
                option("-u", None, NoArg),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "ss",
            "Socket statistics",
            subcommands(&[]),
            vec![
                option("-l", None, NoArg),
                option("-n", None, NoArg),
                option("-p", None, NoArg),
                option("-t", None, NoArg),
                option("-u", None, NoArg),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "ip",
            "IP routing and network device tool",
            subcommands(&["addr", "link", "route", "neigh", "rule", "netns"]),
            vec![],
            NoArg,
        ),
        terminal_fig_spec(
            "ping",
            "Send ICMP echo requests",
            subcommands(&[]),
            vec![
                option("-c", Some("Count"), Value),
                option("-i", Some("Interval"), Value),
                option("-W", Some("Timeout"), Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "traceroute",
            "Trace route to host",
            subcommands(&[]),
            vec![
                option("-I", None, NoArg),
                option("-T", None, NoArg),
                option("-p", Some("Port"), Value),
                option("-m", Some("Max hops"), Value),
            ],
            NoArg,
        ),
        terminal_fig_spec(
            "nc",
            "Netcat",
            subcommands(&[]),
            vec![
                option("-l", Some("Listen"), NoArg),
                option("-p", Some("Port"), Value),
                option("-u", Some("UDP"), NoArg),
                option("-v", Some("Verbose"), NoArg),
                option("-z", Some("Scan"), NoArg),
            ],
            NoArg,
        ),
    ]
}

fn should_run_terminal_path_provider(
    token: &TerminalShellToken,
    active_arg_type: TerminalFigArgType,
) -> bool {
    looks_terminal_path_like(&token.value)
        || matches!(
            active_arg_type,
            TerminalFigArgType::Path | TerminalFigArgType::File | TerminalFigArgType::Directory
        )
}

fn looks_terminal_path_like(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with('~')
        || token.contains('/')
}

fn normalize_terminal_path_token(
    token: &TerminalShellToken,
    cwd: Option<&str>,
) -> Option<TerminalPathParts> {
    let value = token.value.as_str();
    let home = cwd.and_then(infer_terminal_home_from_cwd);
    let expanded = if value.starts_with('~') {
        home.map(|home| format!("{home}{}", &value[1..]))
            .unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    };
    if let Some(slash_index) = expanded.rfind('/') {
        let directory = if slash_index == 0 {
            "/".to_string()
        } else {
            expanded[..slash_index].to_string()
        };
        let query = expanded[slash_index + 1..].to_string();
        let display_prefix = value
            .rfind('/')
            .map(|index| value[..=index].to_string())
            .unwrap_or_default();
        return Some(TerminalPathParts {
            directory,
            query,
            display_prefix,
        });
    }
    let cwd = cwd.unwrap_or(".");
    Some(TerminalPathParts {
        directory: cwd.to_string(),
        query: expanded,
        display_prefix: String::new(),
    })
}

fn infer_terminal_home_from_cwd(cwd: &str) -> Option<String> {
    for prefix in ["/Users/", "/home/"] {
        if let Some(rest) = cwd.strip_prefix(prefix) {
            let user = rest.split('/').next().filter(|user| !user.is_empty())?;
            return Some(format!("{prefix}{user}"));
        }
    }
    cwd.starts_with("/root").then(|| "/root".to_string())
}

fn escape_terminal_path_for_shell(value: &str, quoted: bool) -> String {
    let special = if quoted {
        "\"\\$`"
    } else {
        " \"'\\$`!&|;<>[]{}()*?"
    };
    let mut escaped = String::new();
    for char in value.chars() {
        if special.contains(char) {
            escaped.push('\\');
        }
        escaped.push(char);
    }
    escaped
}

fn load_local_shell_history_commands() -> Vec<String> {
    const MAX_HISTORY_BYTES: usize = 512 * 1024;
    const MAX_COMMANDS: usize = 500;
    static LOCAL_SHELL_HISTORY: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    LOCAL_SHELL_HISTORY
        .get_or_init(|| {
            let Some(home) = std::env::var_os("HOME") else {
                return Vec::new();
            };
            let home = std::path::PathBuf::from(home);
            let files = [
                ".zsh_history",
                ".bash_history",
                ".zhistory",
                ".local/share/fish/fish_history",
            ];
            let mut commands = Vec::new();
            for file in files {
                let path = home.join(file);
                let Ok(mut content) = std::fs::read(&path) else {
                    continue;
                };
                if content.len() > MAX_HISTORY_BYTES {
                    content = content[content.len() - MAX_HISTORY_BYTES..].to_vec();
                }
                let text = String::from_utf8_lossy(&content);
                commands.extend(parse_terminal_history_file(file, &text));
            }
            if commands.len() > MAX_COMMANDS {
                commands[commands.len() - MAX_COMMANDS..].to_vec()
            } else {
                commands
            }
        })
        .clone()
}

fn parse_terminal_history_file(path: &str, content: &str) -> Vec<String> {
    if path.contains("fish_history") {
        return content
            .lines()
            .filter_map(|line| line.strip_prefix("- cmd: "))
            .map(|line| line.replace("\\n", "\n").trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
    }
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(": ")
                && let Some((_, command)) = rest.split_once(';')
            {
                return Some(command.trim().to_string());
            }
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn normalize_terminal_autosuggest_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn terminal_autosuggest_fuzzy_score(command: &str, query: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    if command.starts_with(query) {
        return 1000.0 + query.len() as f64 * 8.0;
    }
    let lower_command = command.to_lowercase();
    let lower_query = query.to_lowercase();
    if lower_command.starts_with(&lower_query) {
        return 850.0 + query.len() as f64 * 6.0;
    }
    if lower_command.contains(&lower_query) {
        return 450.0 + query.len() as f64 * 4.0;
    }

    let query_chars = lower_query.chars().collect::<Vec<_>>();
    if query_chars.is_empty() {
        return 0.0;
    }
    let mut query_index = 0usize;
    let mut score = 0.0;
    for ch in lower_command.chars() {
        if query_index < query_chars.len() && ch == query_chars[query_index] {
            query_index += 1;
            score += 20.0;
        }
    }
    if query_index == query_chars.len() {
        score
    } else {
        0.0
    }
}

fn is_likely_secret_terminal_command(command: &str) -> bool {
    let normalized = command.trim();
    if normalized.is_empty() {
        return false;
    }
    let lower = normalized.to_lowercase();
    lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("passphrase")
        || lower.contains("private_key")
        || lower.contains("private-key")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("access_key")
        || lower.contains("access-key")
        || lower.contains("github_token")
        || lower.contains("openai_api_key")
        || lower.contains("anthropic_api_key")
        || lower.contains("aws_secret_access_key")
        || has_terminal_secret_assignment(normalized)
        || has_terminal_password_flag(normalized)
}

fn has_terminal_secret_assignment(command: &str) -> bool {
    command.split_whitespace().any(|token| {
        let lower = token.to_lowercase();
        token.contains('=')
            && (lower.contains("token")
                || lower.contains("secret")
                || lower.contains("password")
                || lower.contains("api_key")
                || lower.contains("api-key"))
    })
}

fn has_terminal_password_flag(command: &str) -> bool {
    let mut tokens = command.split_whitespace();
    while let Some(token) = tokens.next() {
        if matches!(token, "-p" | "--password" | "--passphrase") && tokens.next().is_some() {
            return true;
        }
    }
    false
}

fn terminal_command_bar_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod terminal_fig_registry_tests {
    use super::*;

    #[test]
    fn built_in_terminal_fig_specs_cover_common_tools() {
        let specs = built_in_terminal_fig_specs();
        assert!(specs.len() >= 55);
        for command in ["jq", "make", "go", "uv", "psql", "tmux", "journalctl", "nc"] {
            assert!(specs.iter().any(|spec| spec.name == command), "{command}");
        }
    }

    #[test]
    fn terminal_fig_config_accepts_subcommand_options() {
        let root: TerminalFigSpecConfigRoot = serde_json::from_str(
            r#"{
                "specs": [{
                    "name": "demo",
                    "description": "Demo",
                    "subcommands": [{
                        "name": "run",
                        "options": [{ "name": "--profile", "args": "value" }]
                    }]
                }]
            }"#,
        )
        .unwrap();
        let TerminalFigSpecConfigRoot::Specs { specs } = root else {
            panic!("expected wrapped specs");
        };
        let spec: TerminalFigSpec = specs.into_iter().next().unwrap().into();
        let run = spec
            .subcommands
            .iter()
            .find(|subcommand| subcommand.name == "run")
            .unwrap();
        assert_eq!(run.options[0].name, "--profile");
        assert_eq!(run.options[0].args, TerminalFigArgType::Value);
    }
}
