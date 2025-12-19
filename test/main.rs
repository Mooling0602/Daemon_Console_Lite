/// This module is used for testing only.
use daemon_console_lite::TerminalApp;

fn handle_input(app: &mut TerminalApp, input: &str, node_counter: &mut usize) -> bool {
    match input {
        "version" => {
            app.info("Demo - v1");
            false
        }
        "exit" => {
            app.info("Exiting...");
            true
        }
        input if input.starts_with("app set-name ") => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() != 3 {
                app.info("Usage: app set-name <name>");
                return false;
            }
            let new_name = parts[2];
            app.app_name = new_name.to_string();
            app.info(&format!("App name set to: {}", new_name));
            false
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

        // ----------------------------------------------------
        // add-node <int> —— Use node_counter
        // ----------------------------------------------------
        input if input.starts_with("add-node ") => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() != 2 {
                app.info("Usage: add-node <number>");
                return false;
            }

            let Ok(count) = parts[1].parse::<usize>() else {
                app.info("add-node 参数必须是整数");
                return false;
            };

            let start = *node_counter + 1;
            let end = *node_counter + count;

            let new_nodes: Vec<String> = (start..=end).map(|i| format!("node{}", i)).collect();
            let ref_nodes: Vec<&str> = new_nodes.iter().map(|s| s.as_str()).collect();

            app.register_tab_completions("", &ref_nodes);
            *node_counter = end;
            app.info(&format!("Added nodes node{} to node{}.", start, end));
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
    let mut node_counter: usize = 0;

    // Configure tab completion display options
    app.tab_option_max_count = 5; // Show up to 5 completion options
    app.tab_option_max_length = 10; // Truncate options longer than 10 characters

    app.enable_tab_completion();

    app.init_terminal("Welcome to Daemon Console Lite!").await?;
    if app.is_tab_completion_enabled() {
        app.info("Tab completion enabled!");
    }

    app.register_tab_completions(
        "",
        &["version", "exit", "help", "config", "app", "add-node"],
    );
    app.register_tab_completions("config", &["start", "stop", "restart", "status", "set"]);
    app.register_tab_completions_with_desc(
        "add-node ",
        &[
            ("", "int: Type should be an integer here."),
            ("", "Type should be an integer here."),
        ],
    );
    app.register_tab_completions_with_desc(
        "app",
        &[("set-name ", "Set the name of the application.")],
    );
    app.register_tab_completions_with_desc(
        "config set",
        &[
            ("port", "Set port number."),
            ("host", "Set host address."),
            ("timeout", "Set timeout."),
        ],
    );

    // Register some long completion options to test truncation
    app.register_tab_completions(
        "",
        &[
            "very-long-command-name",
            "another-extremely-long-option",
            "super-duper-extra-long-command",
        ],
    );

    app.debug("System initialized");

    while let Some(input) = app.read_input().await? {
        if handle_input(&mut app, &input, &mut node_counter) {
            break;
        }
    }

    app.shutdown_terminal("Goodbye!").await?;

    Ok(())
}
