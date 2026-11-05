//! Checkout identity captured on the caller host, including RPC clients.
use std::process::{Command, Stdio};

pub(crate) fn identity(cwd: &str) -> Option<(String, String)> {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-C", cwd])
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).ok())?
    };
    let branch = git(&["symbolic-ref", "--short", "HEAD"])?;
    let origin = git(&["config", "--get", "remote.origin.url"])?;
    Some((github_repo(origin.trim())?, branch.trim().into()))
}

fn github_repo(origin: &str) -> Option<String> {
    let repo = [
        "https://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ]
    .iter()
    .find_map(|prefix| origin.strip_prefix(prefix))?
    .trim_end_matches('/');
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let (owner, name) = repo.split_once('/')?;
    [owner, name]
        .iter()
        .all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
        .then(|| repo.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_origins() {
        for origin in [
            "https://github.com/demo-org/demo.git",
            "git@github.com:demo-org/demo.git",
            "ssh://git@github.com/demo-org/demo",
            "https://github.com/demo-org/demo/",
        ] {
            assert_eq!(github_repo(origin).as_deref(), Some("demo-org/demo"));
        }
        for origin in [
            "https://example.com/demo-org/demo",
            "https://secret@github.com/demo-org/demo",
            "https://github.com/o/r/extra",
            "https://github.com/o/r?token=secret",
        ] {
            assert_eq!(github_repo(origin), None);
        }
    }
}
