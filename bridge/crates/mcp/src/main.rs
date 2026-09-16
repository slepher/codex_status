use std::io::{BufRead, Write};

use bridge_mcp::{handle_request, print_config, McpConfig};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(index) = args.iter().position(|arg| arg == "--print-config") {
        let client = args.get(index + 1).map(String::as_str).unwrap_or("opencode");
        print_config(client);
        return Ok(());
    }
    let cfg = McpConfig::load_default();
    eprintln!(
        "[bridge-mcp] templates: {} (http 127.0.0.1:{})",
        cfg.templates.display(),
        cfg.port
    );
    let runtime = tokio::runtime::Runtime::new()?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                writeln!(
                    stdout,
                    "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32700,\"message\":\"parse error: {err}\"}}}}"
                )?;
                stdout.flush()?;
                continue;
            }
        };
        if let Some(response) = runtime.block_on(handle_request(&cfg, &request)) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
