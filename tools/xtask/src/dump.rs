use std::process::Command;

/// Run a git command and return trimmed stdout, or a fallback string.
fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Dumps repository and build information.
///
/// Uses runtime git commands instead of compile-time shadow_rs to
/// avoid rebuilding xtask on every git operation (commit, branch
/// switch, stash, etc.).
pub fn dump_config() {
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let rev = git(&["rev-parse", "--short", "HEAD"]);
    let author = git(&["log", "-1", "--format=%an"]);
    let email = git(&["log", "-1", "--format=%ae"]);
    let commit_date = git(&["log", "-1", "--format=%ci"]);

    println!(
        "\
* ------------------------
| Build
|   Host   {os}
|   Rustc  {rustc}
| Version Control
|   Branch {branch} ({rev})
|   Author {author} <{email}>
|   Time   {commit_date}
* ------------------------",
        os = std::env::consts::OS,
        rustc = rustc_version(),
    );
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}
