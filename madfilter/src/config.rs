use configparser::ini::Ini;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Config {
    pub mail_domain: String,
    pub max_user_send_per_minute: u32,
    pub max_message_size: usize,
    pub passthrough_senders: Vec<String>,
    pub passthrough_recipients: Vec<String>,
    pub filtermail_smtp_port: u16,
    pub filtermail_smtp_port_incoming: u16,
    pub postfix_reinject_port: u16,
    pub postfix_reinject_port_incoming: u16,
    pub mailboxes_dir: PathBuf,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let mut conf = Ini::new();
        conf.load(path.as_ref().to_str().unwrap()).map_err(|e| anyhow::anyhow!("Failed to load ini: {}", e))?;
        
        let mail_domain = conf.get("params", "mail_domain").ok_or_else(|| anyhow::anyhow!("mail_domain not found"))?;
        
        let max_user_send_per_minute = conf.getuint("params", "max_user_send_per_minute")
            .unwrap_or(Some(60))
            .unwrap_or(60) as u32;
            
        let max_message_size = conf.getuint("params", "max_message_size")
            .unwrap_or(Some(31457280))
            .unwrap_or(31457280) as usize;

        let passthrough_senders = conf.get("params", "passthrough_senders")
            .map(|v: String| v.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>())
            .unwrap_or_default();

        let passthrough_recipients = conf.get("params", "passthrough_recipients")
            .map(|v: String| v.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>())
            .unwrap_or_default();

        let filtermail_smtp_port = conf.getuint("params", "filtermail_smtp_port")
            .unwrap_or(None)
            .ok_or_else(|| anyhow::anyhow!("filtermail_smtp_port not found"))? as u16;

        let filtermail_smtp_port_incoming = conf.getuint("params", "filtermail_smtp_port_incoming")
            .unwrap_or(None)
            .ok_or_else(|| anyhow::anyhow!("filtermail_smtp_port_incoming not found"))? as u16;

        let postfix_reinject_port = conf.getuint("params", "postfix_reinject_port")
            .unwrap_or(None)
            .ok_or_else(|| anyhow::anyhow!("postfix_reinject_port not found"))? as u16;

        let postfix_reinject_port_incoming = conf.getuint("params", "postfix_reinject_port_incoming")
            .unwrap_or(None)
            .ok_or_else(|| anyhow::anyhow!("postfix_reinject_port_incoming not found"))? as u16;

        let mailboxes_dir_str = conf.get("params", "mailboxes_dir")
            .unwrap_or_else(|| format!("/home/vmail/mail/{}", mail_domain));
        let mailboxes_dir = PathBuf::from(mailboxes_dir_str);

        Ok(Config {
            mail_domain,
            max_user_send_per_minute,
            max_message_size,
            passthrough_senders,
            passthrough_recipients,
            filtermail_smtp_port,
            filtermail_smtp_port_incoming,
            postfix_reinject_port,
            postfix_reinject_port_incoming,
            mailboxes_dir,
        })
    }

    pub fn is_incoming_cleartext_ok(&self, addr: &str) -> bool {
        let user_dir = self.mailboxes_dir.join(addr);
        let enforce_path = user_dir.join("enforceE2EEincoming");
        !enforce_path.exists()
    }
}
