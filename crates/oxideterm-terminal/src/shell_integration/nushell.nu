# Use Nushell's own lifecycle and prompt handling instead of adding duplicate hooks.
if (($env.config.shell_integration? | describe) | str starts-with 'record') {
    $env.config.shell_integration.osc133 = true
    $env.config.shell_integration.osc7 = true
} else if (($env.config.shell_integration? | describe) == 'bool') {
    $env.config.shell_integration = true
}
