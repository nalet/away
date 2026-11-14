//! The program, run as each of its parts the way a cluster runs it: started, asked
//! whether it is well, asked for its measures, and ended with the signal a cluster sends.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A part that runs, with the lines it wrote and the addresses they named.
struct Running {
    child: Child,
    lines: Vec<String>,
    listeners: Vec<(String, String)>,
    output: BufReader<std::process::ChildStdout>,
}

/// Starts a part on addresses of its own choosing and waits until it listens on all of them.
fn start(part: &str, listeners: usize) -> Running {
    let mut child = Command::new(env!("CARGO_BIN_EXE_away"))
        .arg(part)
        .env("AWAY_ADDRESS", "127.0.0.1:0")
        .env("AWAY_HEALTH_ADDRESS", "127.0.0.1:0")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the program starts");
    let output = BufReader::new(child.stdout.take().expect("its output"));
    let mut running = Running { child, lines: Vec::new(), listeners: Vec::new(), output };
    while running.listeners.len() < listeners {
        let line = running.next_line().expect("a line for every listener");
        if fact(&line, "event") == "listening" {
            running.listeners.push((fact(&line, "listener"), fact(&line, "on")));
        }
    }
    running
}

/// A part never outlives its test: one whose test failed before it was ended is ended here.
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    fn next_line(&mut self) -> Option<String> {
        let mut line = String::new();
        if self.output.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end().to_string();
        self.lines.push(line.clone());
        Some(line)
    }

    fn address(&self, listener: &str) -> &str {
        &self.listeners.iter().find(|(name, _)| name == listener).unwrap_or_else(|| panic!("no listener {listener}")).1
    }

    /// Ends the part as a cluster does, and gives its exit code and every line it wrote.
    fn end(mut self) -> (i32, Vec<String>) {
        let ended = Command::new("kill").args(["-TERM", &self.child.id().to_string()]).status().expect("kill runs");
        assert!(ended.success(), "the signal was sent");
        while self.next_line().is_some() {}
        let code = self.child.wait().expect("it ends").code().expect("it ends by itself, with a code");
        (code, std::mem::take(&mut self.lines))
    }
}

/// A fact of a line: the text or the number after its name.
fn fact(line: &str, name: &str) -> String {
    let after = line.split(&format!("\"{name}\":")).nth(1).unwrap_or_else(|| panic!("no {name} in {line}"));
    match after.strip_prefix('"') {
        Some(text) => text.split('"').next().unwrap_or_default().to_string(),
        None => after.split([',', '}']).next().unwrap_or_default().to_string(),
    }
}

/// Asks an address and gives the answer's status, its head and its body.
fn ask(address: &str, method: &str, path: &str) -> (u16, String, String) {
    let mut stream = TcpStream::connect(address).expect("it takes the connection");
    stream.set_read_timeout(Some(Duration::from_secs(5))).expect("a time limit");
    write!(stream, "{method} {path} HTTP/1.1\r\nhost: away.test\r\nconnection: close\r\n\r\n").expect("the request goes out");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("an answer comes back");
    let (head, body) = answer.split_once("\r\n\r\n").expect("a head and a body");
    let status = head.split(' ').nth(1).expect("a status").parse().expect("a number");
    (status, head.to_lowercase(), body.to_string())
}

#[test]
fn the_server_answers_on_its_address_and_says_it_is_well() {
    let server = start("server", 2);
    let (status, head, body) = ask(server.address("public"), "GET", "/");
    assert_eq!(status, 200);
    assert!(head.contains("content-type: text/html; charset=utf-8"));
    // the page, or where the program was built without its pages, a page that says so
    let built = away_server::pages::are_built();
    assert!(if built { body.contains("<div id=\"root\"></div>") } else { body.contains("Away is installed and running.") });
    // a browser is told to load nothing from anywhere but Away, and to keep no copy of the page
    let rule = head.lines().find_map(|line| line.strip_prefix("content-security-policy: ")).expect("the rule for the browser");
    assert!(rule.starts_with("default-src 'none'"));
    assert_eq!(rule.contains("script-src 'self'"), built);
    assert!(!rule.contains("unsafe") && !rule.contains("data:") && !rule.contains("http") && !rule.contains('*'));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert!(head.contains("cache-control: no-store"));

    // every address is the page, which says itself when nothing is there; without pages, nothing is
    assert_eq!(ask(server.address("public"), "GET", "/nothing-here").0, if built { 200 } else { 404 });
    assert_eq!(ask(server.address("public"), "GET", "/api/nothing-here").0, 404);
    assert_eq!(ask(server.address("public"), "GET", "/assets/nothing-here.js").0, 404);
    assert_eq!(ask(server.address("public"), "POST", "/").0, 405);
    // what says whether it is well is not on the address Away is reached at
    assert_eq!(ask(server.address("public"), "GET", "/livez").0, 404);
    assert_eq!(ask(server.address("public"), "GET", "/metrics").0, 404);

    assert_eq!(
        ask(server.address("health"), "GET", "/livez"),
        (200, ask(server.address("health"), "GET", "/livez").1, "alive\n".to_string())
    );
    assert_eq!(ask(server.address("health"), "GET", "/readyz").0, 200);
    assert_eq!(ask(server.address("health"), "POST", "/readyz").0, 405);

    let (status, head, measures) = ask(server.address("health"), "GET", "/metrics");
    assert_eq!(status, 200);
    assert!(head.contains("content-type: text/plain; version=0.0.4"));
    assert!(measures.contains("# TYPE away_build_info gauge"));
    assert!(measures.contains(&format!("away_build_info{{part=\"server\",version=\"{}\"", env!("CARGO_PKG_VERSION"))));
    assert!(measures.contains("away_ready{part=\"server\"} 1\n"));
    // of the seven requests above, the page answered one more where it is built
    let (answered, refused) = if built { (2, 5) } else { (1, 6) };
    assert!(measures.contains(&format!("away_requests_total{{part=\"server\",listener=\"public\",answer=\"2xx\"}} {answered}\n")));
    assert!(measures.contains(&format!("away_requests_total{{part=\"server\",listener=\"public\",answer=\"4xx\"}} {refused}\n")));

    let (code, lines) = server.end();
    assert_eq!(code, 0);
    assert_eq!(fact(&lines[0], "event"), "starting");
    assert_eq!(fact(&lines[0], "part"), "server");
    assert_eq!(fact(lines.last().expect("a last line"), "event"), "stopped");
    assert!(lines.iter().any(|line| fact(line, "event") == "stopping" && fact(line, "signal") == "SIGTERM"));
    for line in &lines {
        assert!(line.starts_with("{\"time\":\"20") && line.ends_with('}'), "every line is one object: {line}");
    }
}

#[test]
fn the_gateway_takes_a_sessions_connection_at_its_address_and_says_it_is_well() {
    let gateway = start("gateway", 2);
    // a session's connection is a WebSocket: a request that is none is told so
    let (status, head, body) = ask(gateway.address("sessions"), "GET", "/session");
    assert_eq!(status, 400);
    assert_eq!(body, "A session's connection is a WebSocket.\n");
    let (status, _, body) = ask(gateway.address("sessions"), "GET", "/elsewhere");
    assert_eq!((status, body.as_str()), (404, "No session is at this address.\n"));
    assert!(head.contains("content-security-policy: default-src 'none'"));
    assert_eq!(ask(gateway.address("health"), "GET", "/livez").0, 200);
    assert_eq!(ask(gateway.address("health"), "GET", "/readyz").0, 200);
    assert!(ask(gateway.address("health"), "GET", "/metrics").2.contains("away_ready{part=\"gateway\"} 1\n"));
    let (code, lines) = gateway.end();
    assert_eq!(code, 0);
    assert_eq!(fact(lines.last().expect("a last line"), "event"), "stopped");
}

#[test]
fn the_issuer_has_no_address_of_its_own_and_says_it_is_well() {
    let issuer = start("issuer", 1);
    assert_eq!(issuer.listeners.len(), 1);
    assert_eq!(issuer.listeners[0].0, "health");
    assert_eq!(ask(issuer.address("health"), "GET", "/livez").0, 200);
    assert_eq!(ask(issuer.address("health"), "GET", "/readyz").0, 200);
    assert_eq!(ask(issuer.address("health"), "GET", "/").0, 404);
    assert!(ask(issuer.address("health"), "GET", "/metrics").2.contains("away_ready{part=\"issuer\"} 1\n"));
    let (code, _) = issuer.end();
    assert_eq!(code, 0);
}

#[test]
fn a_connection_that_is_open_does_not_keep_a_part_from_stopping() {
    let server = start("server", 2);
    // a browser that keeps its connection open and asks nothing more
    let mut idle = TcpStream::connect(server.address("public")).expect("it takes the connection");
    write!(idle, "GET / HTTP/1.1\r\nhost: away.test\r\n\r\n").expect("the request goes out");
    let mut first = [0u8; 12];
    idle.read_exact(&mut first).expect("the answer begins");
    assert_eq!(&first, b"HTTP/1.1 200");
    let began = Instant::now();
    let (code, _) = server.end();
    assert_eq!(code, 0);
    assert!(began.elapsed() < Duration::from_secs(5), "it stopped at once, not after the open connection's time ran out");
}

#[test]
fn a_part_that_cannot_listen_says_why_and_ends() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("an address");
    let address = taken.local_addr().expect("its address").to_string();
    let output =
        Command::new(env!("CARGO_BIN_EXE_away")).arg("issuer").env("AWAY_HEALTH_ADDRESS", &address).output().expect("the program runs");
    assert_eq!(output.status.code(), Some(1));
    let lines = String::from_utf8(output.stdout).expect("text");
    let last = lines.lines().last().expect("a line");
    assert_eq!(fact(last, "level"), "error");
    assert_eq!(fact(last, "event"), "could not listen");
    assert_eq!(fact(last, "on"), address);
}

#[test]
fn the_program_says_what_it_is_and_what_it_takes() {
    let run = |words: &[&str]| Command::new(env!("CARGO_BIN_EXE_away")).args(words).output().expect("the program runs");
    let version = run(&["version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with(&format!("away {} (", env!("CARGO_PKG_VERSION"))));
    let help = run(&["help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("away <part>"));
    for wrong in [&[][..], &["desktop"][..], &["server", "now"][..]] {
        let refused = run(wrong);
        assert_eq!(refused.status.code(), Some(2), "{wrong:?}");
        assert!(refused.stdout.is_empty());
        assert!(String::from_utf8_lossy(&refused.stderr).contains("away <part>"));
    }
}
