use super::*;

#[test]
fn test_candidate_resolution() {
    let ws = std::env::current_dir().unwrap();
    let candidates = resolve_candidates(&ws);
    println!("Candidates found: {}", candidates.len());
    for c in &candidates {
        println!("  Candidate: {} -> {} {:?}", c.describe, c.program, c.args);
    }
    assert!(!candidates.is_empty(), "Candidates list must not be empty");
}

#[test]
fn test_installed_backend_spawn() {
    let ws = std::env::current_dir().unwrap().canonicalize().unwrap();
    println!("Testing with canonicalized workspace: {:?}", ws);
    let candidates = resolve_candidates(&ws);
    let installed = candidates
        .iter()
        .find(|c| c.describe == "installed ZCode app runtime");
    if let Some(launch) = installed {
        let mut conn = spawn_connection(launch, &ws).expect("spawn connection");
        let mut ready = false;
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            if let Ok(ev) = conn.events.try_recv() {
                match ev {
                    ConnEvent::Line(line) => {
                        if line.contains(r#""phase":"ready""#) {
                            ready = true;
                            break;
                        }
                    }
                    ConnEvent::Log(log) => {
                        println!("Received log: {log}");
                    }
                    ConnEvent::Exited => {
                        panic!("Backend process exited prematurely");
                    }
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        assert!(ready, "Backend should reach phase: ready");

        let sub_msg = serde_json::json!({
            "id": 1,
            "method": "v4/conversation/subscribe",
            "params": {
                "topic": format!("sessions-index/{}", ws.to_string_lossy()),
                "connectionId": "test-conn",
                "clientMode": "desktop-continuous",
                "visibility": "foreground"
            }
        });
        conn.inbound.send(sub_msg.to_string()).unwrap();

        let mut got_response = false;
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            if let Ok(ev) = conn.events.try_recv() {
                if let ConnEvent::Line(line) = ev {
                    println!("Sub response: {line}");
                    if line.contains(r#""id":1"#) || line.contains("sessions-index") {
                        got_response = true;
                        break;
                    }
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        (conn.kill)();
        assert!(got_response, "Backend should respond to subscription");
    }
}
