use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context};
use serde_json::Value;

pub const REPO_FALLBACK: &str = "aayushpokhrel49/Clipd";

pub fn repo() -> String {
    std::env::var("CLIPD_REPO").unwrap_or_else(|_| REPO_FALLBACK.to_string())
}

pub fn disabled() -> bool {
    std::env::var_os("CLIPD_NO_UPDATE").is_some()
}

pub fn is_dev_build() -> bool {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().contains("/target/"))
        .unwrap_or(false)
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub struct Release {
    pub version: String,
    url: String,
    checksum_url: Option<String>,
}

fn curl_text(url: &str) -> anyhow::Result<Vec<u8>> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            "-H",
            "User-Agent: clipd",
            "-H",
            "Accept: application/vnd.github+json",
            url,
        ])
        .output()
        .context("running curl")?;
    if !out.status.success() {
        return Err(anyhow!(
            "curl {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

fn curl_file(url: &str, dest: &Path) -> anyhow::Result<()> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "300",
            "-H",
            "User-Agent: clipd",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .output()
        .context("running curl")?;
    if !out.status.success() {
        return Err(anyhow!(
            "curl {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Fetch a GitHub API endpoint, returning `None` for HTTP 404 (e.g. no
/// releases published yet) and an error for anything else unexpected.
fn github_api(url: &str) -> anyhow::Result<Option<Value>> {
    let out = Command::new("curl")
        .args([
            "-sSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            "-H",
            "User-Agent: clipd",
            "-H",
            "Accept: application/vnd.github+json",
            "-w",
            "\n%{http_code}",
            url,
        ])
        .output()
        .context("running curl")?;
    if !out.status.success() {
        return Err(anyhow!(
            "curl {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or((text.as_ref(), ""));
    match code.trim() {
        "200" => Ok(Some(
            serde_json::from_str(body).context("parsing GitHub response")?,
        )),
        "404" => Ok(None),
        other => Err(anyhow!("GitHub API returned HTTP {other}")),
    }
}

pub fn check() -> anyhow::Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo());
    let Some(json) = github_api(&url)? else {
        return Ok(None);
    };

    let tag = json
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("release has no tag_name"))?;
    let version = tag.trim_start_matches('v').to_string();
    if !is_newer(&version, current_version()) {
        return Ok(None);
    }

    let mut url = None;
    let mut checksum_url = None;
    if let Some(assets) = json.get("assets").and_then(Value::as_array) {
        for asset in assets {
            let name = asset.get("name").and_then(Value::as_str).unwrap_or_default();
            let dl = asset
                .get("browser_download_url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if dl.is_empty() {
                continue;
            }
            if name == "clipd.sha256" {
                checksum_url = Some(dl.to_string());
            } else if name == "clipd" || name == "clipd-linux-x86_64" {
                url = Some(dl.to_string());
            }
        }
    }

    let url = url.ok_or_else(|| anyhow!("release v{version} has no clipd binary asset"))?;
    Ok(Some(Release {
        version,
        url,
        checksum_url,
    }))
}

/// Download the release binary, verify it, and atomically replace the running
/// executable. Returns the path of the installed binary.
pub fn install(rel: &Release) -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("locating current executable")?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("executable has no parent directory"))?;
    let tmp = dir.join(format!(".clipd-new-{}", std::process::id()));

    let result = (|| -> anyhow::Result<()> {
        curl_file(&rel.url, &tmp)?;

        let data = fs::read(&tmp)?;
        if data.len() < 4 || &data[..4] != b"\x7fELF" {
            return Err(anyhow!("downloaded file is not an ELF executable"));
        }

        if let Some(checksum_url) = &rel.checksum_url {
            if let Ok(raw) = curl_text(checksum_url) {
                let text = String::from_utf8_lossy(&raw);
                let expected = text.split_whitespace().next().unwrap_or_default();
                if !expected.is_empty() {
                    let actual = sha256(&tmp)?;
                    if !actual.eq_ignore_ascii_case(expected) {
                        return Err(anyhow!(
                            "checksum mismatch (expected {expected}, got {actual})"
                        ));
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
        }

        fs::rename(&tmp, &exe).context("replacing executable")?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|_| exe)
}

fn sha256(path: &Path) -> anyhow::Result<String> {
    let out = Command::new("sha256sum").arg(path).output()?;
    if !out.status.success() {
        return Err(anyhow!("sha256sum failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string())
}

/// Spawn a detached helper that waits for this process to exit, then starts
/// the freshly installed binary again.
pub fn restart(path: &Path) {
    let pid = std::process::id();
    let quoted = format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    let script = format!(
        "for _ in $(seq 1 100); do kill -0 {pid} 2>/dev/null || break; sleep 0.1; done; \
         setsid nohup {quoted} start >/dev/null 2>&1 &"
    );
    let _ = Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

pub fn notify(summary: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "clipd", "-i", "edit-copy", summary, body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn parse(v: &str) -> [u64; 3] {
    let mut parts = [0u64; 3];
    for (i, part) in v.split(['.', '-', '+']).take(3).enumerate() {
        parts[i] = part.parse().unwrap_or(0);
    }
    parts
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse(candidate) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.2.0-beta", "0.2.0"));
        assert!(is_newer("0.2.1-beta", "0.2.0"));
    }
}