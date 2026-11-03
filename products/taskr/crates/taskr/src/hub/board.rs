//! GET /board, /board.js, /board.css and /api/board: the hub board for admitted
//! tailnet nodes other than this one. /api/board is `taskr --json board` run as a
//! child of this binary, killed at 10 s, with one 15 s result shared by every Hub.
use super::*;
use axum::body::Bytes;

pub(super) const PATHS: [&str; 4] = ["/board", "/board.js", "/board.css", "/api/board"];
const DEADLINE: Duration = Duration::from_secs(10);
const FRESH: Duration = Duration::from_secs(15);
const OUTPUT_MAX: u64 = 16 << 20;
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'";

/// One cache for the loopback and tailnet Hubs, held across a read so concurrent
/// loads wait for one child and share its result.
static CACHE: Mutex<Option<(tokio::time::Instant, StatusCode, Bytes)>> = Mutex::const_new(None);

/// `get` is whether the method is GET; the request is not held across the read.
pub(super) async fn handle(
    hub: &Hub,
    path: &str,
    get: bool,
    identity: Option<&HubIdentity>,
) -> Response {
    let (Some(identity), Some(server)) = (identity, hub.cfg.identity.as_ref()) else {
        return http_error(StatusCode::FORBIDDEN, "the board serves tailnet nodes only");
    };
    if let Some(refused) = not_this_node(identity, server) {
        return refused;
    }
    if !get {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [("allow", "GET")],
            "Method Not Allowed\n",
        )
            .into_response();
    }
    let (kind, body): (&str, Bytes) = match path {
        "/board" => (
            "text/html; charset=utf-8",
            Bytes::from_static(include_bytes!("board/board.html")),
        ),
        "/board.js" => (
            "text/javascript; charset=utf-8",
            Bytes::from_static(include_bytes!("board/board.js")),
        ),
        "/board.css" => (
            "text/css; charset=utf-8",
            Bytes::from_static(include_bytes!("board/board.css")),
        ),
        _ => {
            let (status, body) = cached(FRESH, || read(hub)).await;
            return (
                status,
                [
                    ("content-type", "application/json; charset=utf-8"),
                    ("content-security-policy", CSP),
                ],
                body,
            )
                .into_response();
        }
    };
    (
        StatusCode::OK,
        [("content-type", kind), ("content-security-policy", CSP)],
        body,
    )
        .into_response()
}

/// The board JSON, read again once the cached one is older than `fresh`; a failed
/// read is cached too, so a failing ledger still costs one child per window.
async fn cached<F: Future<Output = anyhow::Result<Bytes>>>(
    fresh: Duration,
    read: impl FnOnce() -> F,
) -> (StatusCode, Bytes) {
    let mut cache = CACHE.lock().await;
    if let Some((at, status, body)) = &*cache
        && at.elapsed() < fresh
    {
        return (*status, body.clone());
    }
    let (status, body) = match read().await {
        Ok(body) => (StatusCode::OK, body),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from(format!(
                "{}\n",
                compact_json(&json!({"ok":false,"error":"the board could not be read"}))
                    .expect("JSON")
            )),
        ),
    };
    *cache = Some((tokio::time::Instant::now(), status, body.clone()));
    (status, body)
}

async fn read(hub: &Hub) -> anyhow::Result<Bytes> {
    let result = child(hub).await;
    if let Err(e) = &result {
        crate::daemon::rpc_log(&hub.cfg.home, &format!("board read failed: {e}"));
    }
    result
}

async fn child(hub: &Hub) -> anyhow::Result<Bytes> {
    let mut cmd = Command::from(self_command()?);
    cmd.args(["--json", "board"])
        .env_clear()
        .env("HOME", &hub.cfg.home)
        .env("TASKR_DB", &hub.cfg.db_path);
    #[cfg(feature = "contract")]
    if let Ok(now) = std::env::var("TASKR_FROZEN_NOW") {
        cmd.env("TASKR_FROZEN_NOW", now);
    }
    output(cmd, DEADLINE).await
}

/// The child's stdout when it exits 0 within `deadline`; past it the child is killed.
async fn output(mut cmd: Command, deadline: Duration) -> anyhow::Result<Bytes> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child.stdout.take().expect("stdout");
    let run = async {
        let mut out = Vec::new();
        (&mut stdout)
            .take(OUTPUT_MAX + 1)
            .read_to_end(&mut out)
            .await?;
        anyhow::ensure!(
            out.len() as u64 <= OUTPUT_MAX,
            "board output over {OUTPUT_MAX} bytes"
        );
        let status = child.wait().await?;
        anyhow::ensure!(status.success(), "taskr board exited with {status}");
        Ok(Bytes::from(out))
    };
    // A timeout drops the child, and kill_on_drop kills it.
    tokio::time::timeout(deadline, run)
        .await
        .map_err(|_| anyhow::anyhow!("taskr board exceeded {}s", deadline.as_secs_f32()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_read_per_window_shared_by_concurrent_loads() {
        let reads = std::sync::atomic::AtomicUsize::new(0);
        let read = |ok: bool| {
            let reads = &reads;
            move || async move {
                reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                if ok {
                    Ok(Bytes::from_static(b"{\"n\":1}\n"))
                } else {
                    Err(anyhow::anyhow!("ledger busy"))
                }
            }
        };
        *CACHE.lock().await = None;
        // Concurrent loads wait for one read and share it.
        let (a, b, c, d) = tokio::join!(
            cached(FRESH, read(true)),
            cached(FRESH, read(true)),
            cached(FRESH, read(true)),
            cached(FRESH, read(true))
        );
        assert!(
            [a, b, c, d]
                .iter()
                .all(|l| *l == (StatusCode::OK, Bytes::from_static(b"{\"n\":1}\n")))
        );
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Past the window: one new read; its failure is kept for the window too.
        assert_eq!(
            cached(Duration::ZERO, read(false)).await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            cached(FRESH, read(true)).await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_child_past_the_deadline_is_killed() {
        let dir = std::env::temp_dir().join(format!("taskr-board-kill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pidfile = dir.join("pid");
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(format!("echo $$ > {}; exec sleep 30", pidfile.display()));
        let started = tokio::time::Instant::now();
        let err = output(cmd, Duration::from_millis(300)).await.unwrap_err();
        assert!(err.to_string().contains("exceeded"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .to_string();
        // Gone, or a zombie until the runtime reaps it.
        let gone = || {
            std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |s| {
                s.rsplit(')')
                    .next()
                    .unwrap_or("")
                    .trim_start()
                    .starts_with('Z')
            })
        };
        let wait = tokio::time::Instant::now();
        while !gone() {
            assert!(
                wait.elapsed() < Duration::from_secs(5),
                "sleep {pid} survived"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let _ = std::fs::remove_dir_all(&dir);
        let mut ok = Command::new("printf");
        ok.arg("{}");
        assert_eq!(
            output(ok, DEADLINE).await.unwrap(),
            Bytes::from_static(b"{}")
        );
        assert!(output(Command::new("false"), DEADLINE).await.is_err());
    }
}
