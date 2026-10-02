//! SMTP e-mail delivery. Credentials live in their own file and are never part of settings exports.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SmtpCfg {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub from: String,
    pub to: String,
}

fn path() -> std::path::PathBuf {
    crate::data::config_dir().join("smtp.json")
}

impl SmtpCfg {
    pub fn load() -> Self {
        let mut c: Self = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if c.port == 0 {
            c.port = 587;
        }
        c
    }

    pub fn save(&self) -> Result<(), String> {
        let t = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let p = path();
        std::fs::write(&p, t).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn ready(&self) -> bool {
        !self.host.trim().is_empty() && !self.to.trim().is_empty()
    }
}

pub fn send(cfg: &SmtpCfg, subject: &str, body: &str) -> Result<(), String> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};
    if !cfg.ready() {
        return Err("尚未設定郵件伺服器與收件人".into());
    }
    let from = if cfg.from.trim().is_empty() { cfg.user.trim() } else { cfg.from.trim() };
    let mut mb = Message::builder().from(from.parse().map_err(|e| format!("寄件人格式錯誤:{e}"))?).subject(subject);
    for to in cfg.to.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        mb = mb.to(to.parse().map_err(|e| format!("收件人「{to}」格式錯誤:{e}"))?);
    }
    let msg = mb.body(body.to_string()).map_err(|e| e.to_string())?;
    let host = cfg.host.trim();
    let b = if cfg.port == 465 { SmtpTransport::relay(host) } else { SmtpTransport::starttls_relay(host) }
        .map_err(|e| format!("伺服器設定錯誤:{e}"))?;
    let mut b = b.port(cfg.port).timeout(Some(Duration::from_secs(20)));
    if !cfg.user.is_empty() {
        b = b.credentials(Credentials::new(cfg.user.clone(), cfg.pass.clone()));
    }
    b.build().send(&msg).map(|_| ()).map_err(|e| format!("寄送失敗:{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_is_a_clean_error() {
        assert!(SmtpCfg::default().send_err());
    }

    impl SmtpCfg {
        fn send_err(&self) -> bool {
            send(self, "s", "b").is_err()
        }
    }

    #[test]
    fn bad_recipient_is_reported_without_network() {
        let c = SmtpCfg { host: "localhost".into(), port: 587, from: "a@b.co".into(), to: "not an address".into(), ..Default::default() };
        assert!(send(&c, "s", "b").unwrap_err().contains("格式錯誤"));
    }
}
