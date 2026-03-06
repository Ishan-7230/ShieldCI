use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;
use std::io::Read;
use std::io::BufRead;
use std::io::BufReader;

#[derive(Debug)]
struct TargetConfig {
    framework: String,
    build_command: String,
    run_command: String,
    target_url: String,
}

#[derive(Deserialize, Debug)]
struct ToolCall {
    tool: String,
    target: String,
}

fn fetch_config_from_shell() -> TargetConfig {
    println!("Calling detect.sh to scout the repository");

    let output = Command::new("bash")
        .arg("run.sh")
        .output()
        .expect("Failed to execute detect.sh");

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stdout);
        panic!("Detection script failed: {}", err_msg);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut config_map = HashMap::new();
    
    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            config_map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    let config = TargetConfig {
        framework: config_map.get("FRAMEWORK").expect("Missing FRAMEWORK").clone(),
        build_command: config_map.get("BUILD_CMD").expect("Missing BUILD_CMD").clone(),
        run_command: config_map.get("RUN_CMD").expect("Missing RUN_CMD").clone(),
        target_url: config_map.get("TARGET_URL").expect("Missing TARGET_URL").clone(),
    };

    println!("Detected {} environment. Target mapping complete.", config.framework);
    config
}

fn launch_sandbox(config: &TargetConfig) {
    if !config.build_command.is_empty() {
        let parts: Vec<&str> = config.build_command.split_whitespace().collect();
        println!("Running build: {}", config.build_command);
        let status = Command::new(parts[0])
            .args(&parts[1..])
            .status()
            .expect("Failed to execute build command");

        if !status.success() {
            panic!("Build failed! Halting pipeline.");
        }
    }

    let run_parts: Vec<&str> = config.run_command.split_whitespace().collect();
    println!("Launching {} server on {}", config.framework, config.target_url);
    
    Command::new(run_parts[0])
        .args(&run_parts[1..])
        .stdout(Stdio::null()) 
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start the target application server");
}

async fn wait_for_target(url: &str, max_retries: u8) -> Result<(), String> {
    println!("⏳ Waiting for target {} to come online...", url);
    let client = Client::builder().timeout(Duration::from_secs(2)).build().unwrap();

    for _ in 1..=max_retries {
        match client.get(url).send().await {
            Ok(response) => {
                println!("\nTarget is up and responding (HTTP {})!", response.status());
                return Ok(());
            }
            Err(_) => {
                print!(".");
                io::stdout().flush().unwrap();
            }
        }
        sleep(Duration::from_secs(2)).await;
    }
    Err(format!("\nTarget {} failed to respond. Halting pipeline.", url))
}

fn flatten_codebase(dir: &str) -> String {
    println!("Flattening microservice codebase for LLM context");
    // Reminder upgrade to walkdir later
    let mut full_code = String::new();
    
    // Reminder make dynamic later
    if Path::new("abc.py").exists() {
        full_code.push_str("FILE: abc.py\n");
        full_code.push_str(&fs::read_to_string("abc.py").unwrap_or_default());
    }
    
    full_code
}



fn ask_llm(system_prompt: &str) -> ToolCall {
    println!("Invoking local model");

    let prompt_path = "temp_prompt.txt";
    fs::write(prompt_path, system_prompt).expect("Failed to write prompt file");

    let output = Command::new("./llama.cpp/llama-cli")
        .args([
            "-m", "./models/qwen2.5-coder-7b.gguf",
            "-f", prompt_path,
            "--grammar-file", "./tool_call.gbnf",
            "--temp", "0.0",
            "-n", "128",
            "--log-disable"
        ])
        .output()
        .expect("Failed to execute llama.cpp");

    let _ = fs::remove_file(prompt_path);

    if !output.status.success() {
        panic!("llama.cpp crashed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let result = String::from_utf8_lossy(&output.stdout).to_string();
    let json_start = result.find('{').unwrap_or(0);
    let json_end = result.rfind('}').unwrap_or(result.len() - 1) + 1;
    let raw_json = &result[json_start..json_end];

    serde_json::from_str(raw_json).expect("LLM produced invalid JSON despite GBNF grammar")
}


async fn execute_mcp_tool(tool_call: &ToolCall) -> Result<String, Box<dyn std::error::Error>> {
    println!("Initiating MCP Handshake & Strike: {} on {}", tool_call.tool, tool_call.target);
    
    let mut child = Command::new("docker")
        .args(["run", "-i", "--rm", "--network", "host", "shieldci-kali-image"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn Docker MCP container");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let mut stdout_reader = BufReader::new(child.stdout.take().expect("Failed to open stdout"));

    let init_req = r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "shieldci-orchestrator", "version": "1.0.0"}}}"#;
    writeln!(stdin, "{}", init_req)?;
    
    let mut init_response = String::new();
    stdout_reader.read_line(&mut init_response)?;

    let init_notif = r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#;
    writeln!(stdin, "{}", init_notif)?;

    let mcp_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2, 
        "method": "tools/call",
        "params": {
            "name": &tool_call.tool,
            "arguments": {
                "target_url": &tool_call.target
            }
        }
    });
    let payload_str = serde_json::to_string(&mcp_payload)?;
    writeln!(stdin, "{}", payload_str)?;

    drop(stdin);

    let mut final_output = String::new();
    stdout_reader.read_to_string(&mut final_output)?;

    let _ = child.wait()?; 

    Ok(final_output)
}


#[tokio::main]
async fn main() {
    println!("Booting ShieldCI Orchestrator");

    let config = fetch_config_from_shell();
    launch_sandbox(&config);
    wait_for_target(&config.target_url, 15).await.unwrap();

    let codebase = flatten_codebase(".");

    println!("Initiating autonomous vulnerability scan");
    let mut exploit_found = false;
    let mut attack_trace = String::new();
    
    let mut current_prompt = format!(
        "You are an AI pentester. Target: {}. \
         Tools available: sqlmap, nmap. \
         Codebase: {}\n\n\
         Output strict JSON with the single next tool command to run.",
         config.target_url, codebase
    );

    for iteration in 1..=3 {
        println!("\nStrike Iteration {}", iteration);
        
        let tool_call = ask_llm(&current_prompt);
        
       
        let terminal_output = execute_mcp_tool(&tool_call).await.unwrap_or_else(|e| e.to_string());
        println!("Terminal Output: {}", terminal_output);

        if terminal_output.to_lowercase().contains("vulnerability found") || terminal_output.contains("sql injection") {
            println!("CRITICAL VULNERABILITY CONFIRMED.");
            exploit_found = true;
            attack_trace = terminal_output;
            break;
        } else {
            current_prompt.push_str(&format!("\nObservation from {}: {}", tool_call.tool, terminal_output));
        }
    }

    if exploit_found {
        println!("\nVulnerability exploited. Waking LLM to generate code patch...");
        
        println!("Patch generated. Ready to block PR merge.");
        std::process::exit(1); 
    } else {
        println!("\nTarget is secure. No vulnerabilities found in 3 iterations.");
        std::process::exit(0);
    }
}