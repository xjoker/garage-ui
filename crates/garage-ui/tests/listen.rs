// Starts the release-style binary on a chosen port and checks where it listens and what it
// writes into DATA_DIR. No Garage is needed: the UI boots and answers /healthz without one.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn work_dir(label: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("garage-ui-listen-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn healthz(port: u16) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut text = String::new();
    stream.read_to_string(&mut text).ok()?;
    Some(text)
}

fn start(dir: &Path, data: &Path, port: Option<u16>) -> Server {
    let mut command = Command::new(env!("CARGO_BIN_EXE_garage-ui"));
    command
        .env_clear()
        .env("DATA_DIR", data)
        .env("HOST", "127.0.0.1")
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(port) = port {
        command.env("PORT", port.to_string());
    }
    Server(command.spawn().unwrap())
}

fn wait_ready(server: &mut Server, port: u16) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            panic!("garage-ui exited before listening on {port}: {status}");
        }
        if let Some(text) = healthz(port)
            && text.starts_with("HTTP/1.1 200")
        {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "garage-ui did not listen on {port}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn listens_on_port_and_keeps_data_dir_private() {
    let dir = work_dir("port");
    let data = dir.join("data");
    let port = free_port();
    let mut server = start(&dir, &data, Some(port));
    let health = wait_ready(&mut server, port);
    assert!(health.contains("\"status\":\"ok\""), "{health}");

    #[cfg(unix)]
    {
        assert_eq!(mode(&data), 0o700, "data directory");
        for name in ["garage-ui.sqlite", "session.key"] {
            assert_eq!(mode(&data.join(name)), 0o600, "{name}");
        }
        for name in ["garage-ui.sqlite-wal", "garage-ui.sqlite-shm"] {
            let path = data.join(name);
            if path.exists() {
                assert_eq!(mode(&path), 0o600, "{name}");
            }
        }
    }
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn an_existing_open_data_dir_is_restricted() {
    use std::os::unix::fs::PermissionsExt;
    let dir = work_dir("existing");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
    let port = free_port();
    let mut server = start(&dir, &data, Some(port));
    wait_ready(&mut server, port);
    assert_eq!(mode(&data), 0o700);
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_invalid_port_stops_startup() {
    let dir = work_dir("invalid");
    let output = Command::new(env!("CARGO_BIN_EXE_garage-ui"))
        .env_clear()
        .env("DATA_DIR", dir.join("data"))
        .env("PORT", "70000")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("PORT must be a TCP port from 1 to 65535: 70000"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn an_existing_setgid_data_dir_keeps_setgid() {
    use std::os::unix::fs::PermissionsExt;
    let dir = work_dir("setgid");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o2775)).unwrap();
    let port = free_port();
    let mut server = start(&dir, &data, Some(port));
    wait_ready(&mut server, port);
    let full = std::fs::metadata(&data).unwrap().permissions().mode() & 0o7777;
    assert_eq!(full, 0o2700, "only the permission bits change");
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

// A data directory that other users can write to, and that this process may not change, stops
// startup before anything is written there. Without root, the only such directory a test can
// rely on is a shared one owned by another user, like `/tmp` (root-owned, mode 1777). The test
// is skipped, with a message, when the temporary directory is not like that (for example when
// the tests run as root) or already holds a file the UI would use.
#[cfg(unix)]
#[test]
fn a_data_dir_other_users_can_write_stops_startup() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let shared = std::env::temp_dir();
    let own = work_dir("shared-owner");
    let me = std::fs::metadata(&own).unwrap().uid();
    let _ = std::fs::remove_dir_all(&own);
    let before = std::fs::metadata(&shared).unwrap();
    let planted = ["garage-ui.sqlite", "session.key", ".write-probe"]
        .iter()
        .any(|name| shared.join(name).exists());
    if before.uid() == me || before.mode() & 0o002 == 0 || planted {
        eprintln!(
            "SKIPPED: {} is not another user's directory that others can write to",
            shared.display()
        );
        return;
    }
    let cleanup = || {
        for name in [
            "garage-ui.sqlite",
            "garage-ui.sqlite-wal",
            "garage-ui.sqlite-shm",
        ] {
            let _ = std::fs::remove_file(shared.join(name));
        }
        let _ = std::fs::remove_file(shared.join("session.key"));
    };
    let check = |label: &str, args: &[&str], extra: &[(&str, String)]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_garage-ui"));
        command
            .env_clear()
            .env("DATA_DIR", &shared)
            .env("HOST", "127.0.0.1")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in extra {
            command.env(key, value);
        }
        let mut child = Server(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                drop(child);
                cleanup();
                panic!("{label}: garage-ui kept running in a directory others can write to");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut text = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let wrote = ["garage-ui.sqlite", "session.key"]
            .iter()
            .any(|name| shared.join(name).exists());
        if wrote {
            cleanup();
        }
        assert!(!status.success(), "{label}: {text}");
        assert!(!wrote, "{label}: wrote into the shared directory");
        assert!(
            text.contains("is writable by other users") && text.contains("Refusing to start"),
            "{label}: {text}"
        );
    };
    check("service", &[], &[("PORT", free_port().to_string())]);
    let after = std::fs::metadata(&shared).unwrap();
    assert_eq!(
        after.permissions().mode() & 0o7777,
        before.permissions().mode() & 0o7777,
        "the shared directory's mode changed"
    );
}
