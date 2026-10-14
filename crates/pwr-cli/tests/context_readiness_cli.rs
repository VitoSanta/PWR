//! Opening a headless session must prepare and persist its requested window.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn opening_a_session_prepares_the_window_without_an_app_settings_request() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let models = root.path().join("models");
    let model_ref = "mlx-community/gemma-4-12B-it-4bit";
    let model = models.join(model_ref);
    std::fs::create_dir_all(&model).unwrap();
    std::fs::create_dir_all(workspace.join(".pwr")).unwrap();
    std::fs::write(
        model.join("config.json"),
        json!({
            "model_type":"gemma4_unified", "max_position_embeddings":262144
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(model.join("model.safetensors"), [1_u8]).unwrap();
    let config_path = workspace.join(".pwr/chat-config.json");
    std::fs::write(
        &config_path,
        json!({
            "backend":"mlx", "model":model_ref, "context_setting":2048
        })
        .to_string(),
    )
    .unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_pwr"))
            .args(["serve", "--stdio"])
            .env("PWR_MLX_MODELS", &models)
            .env("PWR_HOME", root.path().join("personal"))
            // Preparing a window must not load weights or require an interpreter.
            .env("PWR_MLX_PYTHON", root.path().join("no-interpreter"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = process.0.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && send.send(value).is_err()
            {
                break;
            }
        }
    });
    for (id, method, params) in [
        (
            1,
            "initialize",
            json!({"protocolVersion":1,"clientCapabilities":{}}),
        ),
        (2, "session/new", json!({"cwd":workspace,"mcpServers":[]})),
    ] {
        let stdin = process.0.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        stdin.flush().unwrap();
        loop {
            let reply = receive
                .recv_timeout(Duration::from_secs(30))
                .expect("CLI reply");
            if reply["id"] == id {
                assert!(reply.get("error").is_none(), "{reply}");
                break;
            }
        }
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    assert_eq!(
        saved["context_tokens"], 2048,
        "the requested setting must reach the runtime file before the first turn; got {saved}"
    );
    assert_eq!(saved["context_setting"], 2048);
}
