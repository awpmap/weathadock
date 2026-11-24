//! In-app updates from GitHub Releases. The app asks GitHub for the latest release, downloads
//! its installer over HTTPS, checks it against the SHA-256 GitHub publishes for the file, and
//! runs it when the person chooses "Restart to update".

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

const LATEST: &str = "https://api.github.com/repos/awpmap/weathadock/releases/latest";

#[derive(Default)]
pub struct Updater {
    state: Mutex<UpdateState>,
    busy: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
struct UpdateState {
    status: &'static str,
    new_version: Option<String>,
    percent: Option<u32>,
    error: Option<String>,
    checked_at: Option<String>,
    installer: Option<PathBuf>,
}

impl Default for UpdateState {
    fn default() -> Self {
        // Development builds have no installer to replace.
        let status = if cfg!(debug_assertions) || !cfg!(windows) { "unsupported" } else { "idle" };
        UpdateState { status, new_version: None, percent: None, error: None, checked_at: None, installer: None }
    }
}

fn version_parts(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Deletes the installers kept from earlier updates, and nothing else. One that is still
/// running cannot be deleted and is left for the next check.
async fn clear_old_installers(dir: &std::path::Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else { return };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("weathadock-") && name.ends_with("-setup.exe") {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

impl Updater {
    pub fn public(&self, app: &AppHandle) -> Value {
        let s = self.state.lock().unwrap().clone();
        json!({
            "version": app.package_info().version.to_string(), "status": s.status, "newVersion": s.new_version,
            "percent": s.percent, "error": s.error, "checkedAt": s.checked_at,
        })
    }

    pub fn is_ready(&self) -> Option<String> {
        let s = self.state.lock().unwrap();
        (s.status == "ready").then(|| s.new_version.clone().unwrap_or_default())
    }

    fn set(&self, app: &AppHandle, change: impl FnOnce(&mut UpdateState)) {
        change(&mut self.state.lock().unwrap());
        let _ = app.emit("sky:updateState", self.public(app));
    }

    /// Check for a newer release and download it. Safe to call at any time.
    pub async fn check(&self, app: &AppHandle) -> Value {
        let status = self.state.lock().unwrap().status;
        if status == "unsupported" || status == "ready" {
            return self.public(app);
        }
        let Ok(_guard) = self.busy.try_lock() else { return self.public(app) };
        self.set(app, |s| {
            s.status = "checking";
            s.error = None;
        });
        if let Err(message) = self.run(app).await {
            self.set(app, |s| {
                s.status = "error";
                s.error = Some(message.chars().take(200).collect());
            });
        }
        self.public(app)
    }

    async fn run(&self, app: &AppHandle) -> Result<(), String> {
        // No installer is waiting to be run at this point, so whatever is in the folder has
        // already been installed or was never used. Each one is several megabytes.
        if let Ok(cache) = app.path().app_cache_dir() {
            clear_old_installers(&cache.join("updates")).await;
        }
        let net = app.state::<crate::net::Net>();
        let release: Value = net
            .client()
            .get(LATEST)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("Could not reach GitHub: {e}"))?
            .error_for_status()
            .map_err(|e| format!("GitHub answered with an error: {e}"))?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let latest = release["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
        let current = app.package_info().version.to_string();
        if latest.is_empty() || version_parts(&latest) <= version_parts(&current) {
            self.set(app, |s| {
                s.status = "up-to-date";
                s.new_version = None;
                s.checked_at = Some(now_iso());
            });
            return Ok(());
        }
        let asset = release["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"].as_str().map_or(false, |n| n.to_lowercase().ends_with("-setup.exe")))
            .ok_or("The new release has no installer attached yet. Try again in a few minutes.")?;
        let url = asset["browser_download_url"].as_str().ok_or("The release is missing its download link.")?;
        if !url.starts_with("https://github.com/awpmap/weathadock/releases/download/") {
            return Err("The release points at an unexpected download address.".into());
        }
        let expected = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase);
        let total = asset["size"].as_u64().unwrap_or(0);
        self.set(app, |s| {
            s.status = "downloading";
            s.new_version = Some(latest.clone());
            s.percent = Some(0);
        });

        // Installers are large, so this request gets its own generous timeout.
        let mut res = net
            .client()
            .get(url)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| format!("Download failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Download failed: {e}"))?;
        let mut bytes: Vec<u8> = Vec::with_capacity(total as usize);
        let mut hasher = Sha256::new();
        let mut shown = 0;
        while let Some(chunk) = res.chunk().await.map_err(|e| format!("Download interrupted: {e}"))? {
            hasher.update(&chunk);
            bytes.extend_from_slice(&chunk);
            if total > 0 {
                let percent = (bytes.len() as u64 * 100 / total).min(100) as u32;
                if percent >= shown + 2 {
                    shown = percent;
                    self.set(app, |s| s.percent = Some(percent));
                }
            }
        }
        let actual = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
        match expected {
            Some(e) if e != actual => return Err("The downloaded update did not match its published checksum, so it was discarded.".into()),
            Some(_) => {}
            None => {
                if total > 0 && bytes.len() as u64 != total {
                    return Err("The downloaded update was incomplete.".into());
                }
            }
        }
        let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");
        tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
        let file = dir.join(format!("WeathaDock-{latest}-setup.exe"));
        tokio::fs::write(&file, &bytes).await.map_err(|e| format!("Could not save the update: {e}"))?;
        self.set(app, |s| {
            s.status = "ready";
            s.percent = Some(100);
            s.checked_at = Some(now_iso());
            s.installer = Some(file);
        });
        Ok(())
    }

    /// Run the downloaded installer (passive mode, relaunch afterwards) and quit.
    pub fn install(&self, app: &AppHandle) {
        let installer = self.state.lock().unwrap().installer.clone();
        let Some(path) = installer else { return };
        match std::process::Command::new(&path).args(["/P", "/R"]).spawn() {
            Ok(_) => app.exit(0),
            Err(e) => self.set(app, |s| {
                s.status = "error";
                s.error = Some(format!("Could not start the installer: {e}"));
                s.installer = None;
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(version_parts("v1.10.0") > version_parts("1.9.3"));
        assert!(version_parts("1.1.0") <= version_parts("1.1.0"));
        assert!(version_parts("1.0.1") < version_parts("1.1.0"));
    }
}
