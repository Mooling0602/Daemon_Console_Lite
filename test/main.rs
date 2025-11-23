use daemon_console_lite::TerminalApp;

fn handle_input(app: &mut TerminalApp, input: &str) -> bool {
    match input {
        "version" => {
            app.info("Demo - v1");
            false
        }
        "exit" => {
            app.info("Exiting...");
            true
        }
        input if input.starts_with("config start") => {
            app.info("Starting service...");
            false
        }
        input if input.starts_with("config stop") => {
            app.info("Stopping service...");
            false
        }
        input if input.starts_with("config restart") => {
            app.info("Restarting service...");
            false
        }
        input if input.starts_with("config set port") => {
            app.info("Setting port configuration...");
            false
        }
        input if input.starts_with("config set host") => {
            app.info("Setting host configuration...");
            false
        }
        _ => {
            app.info(&format!("You entered: {}", input));
            false
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = TerminalApp::new();
    app.enable_tab_completion();
    app.register_tab_completions("", &["version", "exit", "help", "config"]);
    app.register_tab_completions("config", &["start", "stop", "restart", "status", "set"]);
    app.register_tab_completions_with_desc(
        "config set",
        &[
            ("port", "Set port number."),
            ("host", "Set host address."),
            ("timeout", "Set timeout."),
        ],
    );
    app.init_terminal("Welcome to Daemon Console Lite!").await?;
    app.info("Tab completion enabled! Use Alt+Left/Right to select, Tab to complete.");
    app.debug("System initialized");

    while let Some(input) = app.read_input().await? {
        if handle_input(&mut app, &input) {
            break;
        }
    }

    app.shutdown_terminal("Goodbye!").await?;

    Ok(())
}
