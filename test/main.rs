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
        input if input.starts_with("set-name ") => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() != 2 {
                app.info("Usage: set-name <name>");
                return false;
            }
            let new_name = parts[1];
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
        // add-node <N> —— 使用外部计数器 node_counter
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

            // 起始 node index
            let start = *node_counter + 1;
            let end = *node_counter + count;

            // 批量生成 nodeX
            let new_nodes: Vec<String> = (start..=end).map(|i| format!("node{}", i)).collect();

            // 转换成 Vec<&str>
            let ref_nodes: Vec<&str> = new_nodes.iter().map(|s| s.as_str()).collect();

            // 注册到 root
            app.register_tab_completions("", &ref_nodes);

            // 更新计数器
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
    let mut node_counter: usize = 0; // 维护 node 数量计数器

    app.enable_tab_completion();
    app.register_tab_completions("", &["version", "exit", "help", "config"]);
    app.register_tab_completions("config", &["start", "stop", "restart", "status", "set"]);
    app.register_tab_completions_with_desc(
        "app",
        &[("set-name", "Set the name of the application.")],
    );
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
        if handle_input(&mut app, &input, &mut node_counter) {
            break;
        }
    }

    app.shutdown_terminal("Goodbye!").await?;

    Ok(())
}
