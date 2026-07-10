fn git_exact_tag() -> Option<String> {
    std::process::Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
}

fn git_short_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=5", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_string())
        .unwrap_or_default()
}

fn build_revision() -> String {
    if let Ok(revision) = std::env::var("PIPE_DECK_BUILD_REVISION") {
        let revision = revision.trim().to_string();
        if !revision.is_empty() {
            return revision;
        }
    }

    if let Some(tag) = git_exact_tag() {
        return tag;
    }

    git_short_hash()
}

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-env-changed=PIPE_DECK_BUILD_REVISION");
    println!(
        "cargo:rustc-env=PIPE_DECK_BUILD_REVISION={}",
        build_revision()
    );
    tauri_build::build()
}
