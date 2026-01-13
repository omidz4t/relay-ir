use crate::config::Config;
use crate::filter::{check_encrypted, is_securejoin, ENCRYPTION_NEEDED_523};
use crate::rate_limit::SendRateLimiter;
use mail_parser::{Message, MimeHeaders};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

pub struct SmtpProxy {
    config: Arc<Config>,
    rate_limiter: Arc<SendRateLimiter>,
    mode: String,
}

impl SmtpProxy {
    pub fn new(config: Arc<Config>, rate_limiter: Arc<SendRateLimiter>, mode: String) -> Self {
        Self {
            config,
            rate_limiter,
            mode,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let port = if self.mode == "outgoing" {
            self.config.filtermail_smtp_port
        } else {
            self.config.filtermail_smtp_port_incoming
        };

        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
        eprintln!("Serving {} on 127.0.0.1:{}", self.mode, port);

        loop {
            let (stream, _) = listener.accept().await?;
            let config = Arc::clone(&self.config);
            let rate_limiter = Arc::clone(&self.rate_limiter);
            let mode = self.mode.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, config, rate_limiter, mode).await {
                    eprintln!("Error handling connection: {}", e);
                }
            });
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    config: Arc<Config>,
    rate_limiter: Arc<SendRateLimiter>,
    mode: String,
) -> anyhow::Result<()> {
    let peer_addr = stream.peer_addr()?;
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    writer.write_all(b"220 localhost ESMTP\r\n").await?;
    eprintln!("SMTP: {} New connection from {}", mode, peer_addr);

    let mut mail_from = String::new();
    let mut rcpt_tos = Vec::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }

        let cmd = line.trim();
        let cmd_upper = cmd.to_uppercase();

        if cmd_upper.starts_with("HELO") || cmd_upper.starts_with("EHLO") {
            writer.write_all(b"250-localhost\r\n250-PIPELINING\r\n250-SIZE 31457280\r\n250 OK\r\n").await?;
        } else if cmd_upper.starts_with("MAIL FROM:") {
            let addr = extract_addr(cmd, "MAIL FROM:");
            mail_from = addr.clone();
            
            if mode == "outgoing" {
                if !rate_limiter.is_sending_allowed(&mail_from, config.max_user_send_per_minute) {
                    eprintln!("SMTP: Rate limit exceeded for {}", mail_from);
                    writer.write_all(format!("450 4.7.1: Too much mail from {}\r\n", mail_from).as_bytes()).await?;
                    continue;
                }
            }
            eprintln!("SMTP: MAIL FROM:<{}>", mail_from);
            writer.write_all(b"250 OK\r\n").await?;
        } else if cmd_upper.starts_with("RCPT TO:") {
            let addr = extract_addr(cmd, "RCPT TO:");
            eprintln!("SMTP: RCPT TO:<{}>", addr);
            rcpt_tos.push(addr);
            writer.write_all(b"250 OK\r\n").await?;
        } else if cmd_upper == "DATA" {
            writer.write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n").await?;
            
            let mut data = Vec::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await?;
                if n == 0 {
                    break;
                }
                if line == ".\r\n" || line == ".\n" {
                    break;
                }
                let mut content = line.as_str();
                if content.starts_with('.') {
                    content = &content[1..];
                }
                data.extend_from_slice(content.as_bytes());
            }

            // Processing DATA
            let msg = mail_parser::MessageParser::default().parse(&data).ok_or_else(|| anyhow::anyhow!("Failed to parse message"))?;
            let error = check_data(&msg, &mail_from, &rcpt_tos, &config, &mode);
            
            if let Some(err_msg) = error {
                eprintln!("SMTP: Rejecting data: {}", err_msg);
                writer.write_all(format!("{}\r\n", err_msg).as_bytes()).await?;
            } else {
                // Re-inject
                let reinject_port = if mode == "outgoing" {
                    config.postfix_reinject_port
                } else {
                    config.postfix_reinject_port_incoming
                };
                
                eprintln!("SMTP: Re-injecting {} bytes to port {}", data.len(), reinject_port);
                match reinject(&mail_from, &rcpt_tos, &data, reinject_port).await {
                    Ok(_) => {
                        writer.write_all(b"250 OK\r\n").await?;
                    }
                    Err(e) => {
                        eprintln!("SMTP: Re-inject failed: {}", e);
                        writer.write_all(b"451 Error re-injecting mail\r\n").await?;
                    }
                }
            }
            // Reset for next message if needed (though usually one per connection in simple cases)
            mail_from.clear();
            rcpt_tos.clear();
        } else if cmd_upper == "QUIT" {
            writer.write_all(b"221 Bye\r\n").await?;
            break;
        } else if cmd_upper == "RSET" {
            mail_from.clear();
            rcpt_tos.clear();
            writer.write_all(b"250 OK\r\n").await?;
        } else if cmd_upper == "NOOP" {
            writer.write_all(b"250 OK\r\n").await?;
        } else {
            writer.write_all(b"500 Unknown command\r\n").await?;
        }
    }

    Ok(())
}

fn extract_addr(cmd: &str, prefix: &str) -> String {
    let mut addr = cmd[prefix.len()..].trim();
    // Remove options like SMTPUTF8 or SIZE first
    if let Some((a, _)) = addr.split_once(' ') {
        addr = a;
    }
    if addr.starts_with('<') && addr.ends_with('>') {
        addr = &addr[1..addr.len() - 1];
    }
    addr.to_string()
}

fn check_data(
    msg: &Message,
    mail_from: &str,
    rcpt_tos: &[String],
    config: &Config,
    mode: &str,
) -> Option<String> {
    let outgoing = mode == "outgoing";
    let is_encrypted = check_encrypted(msg, outgoing);
    let is_sj = is_securejoin(msg);

    if outgoing {
        let from_header = msg.from().and_then(|f| f.first()?.address.as_ref());
        if let Some(from_addr) = from_header {
            if mail_from.to_lowercase() != from_addr.to_lowercase() {
                return Some(format!("500 Invalid FROM <{}> for <{}>", from_addr, mail_from));
            }
        }

        if is_encrypted || is_sj {
            return None;
        }

        if config.passthrough_senders.iter().any(|s| s == mail_from) {
            return None;
        }

        // Allow self-sent Autocrypt Setup Message
        if rcpt_tos.len() == 1 && rcpt_tos[0].to_lowercase() == mail_from.to_lowercase() {
            if msg.subject() == Some("Autocrypt Setup Message") {
                // In Python: if message.get_content_type() == "multipart/mixed": return
                if msg.is_content_type("multipart", "mixed") {
                    return None;
                }
            }
        }

        for rcpt in rcpt_tos {
            if recipient_matches_passthrough(rcpt, &config.passthrough_recipients) {
                continue;
            }
            eprintln!("REJECT: Outgoing unencrypted mail rejected (Subject: {:?})", msg.subject());
            return Some(ENCRYPTION_NEEDED_523.to_string());
        }
    } else {
        // Incoming
        if is_encrypted || is_sj {
            return None;
        }

        // Mailer-daemon
        let auto_submitted = msg.header("Auto-Submitted").and_then(|h| h.as_text());
        if auto_submitted.is_some() {
            let from_header = msg.from().and_then(|f| f.first()?.address.as_ref());
            if let Some(from_addr) = from_header {
                if from_addr.to_lowercase().starts_with("mailer-daemon@") {
                    if msg.is_content_type("multipart", "report") {
                        return None;
                    }
                }
            }
        }

        for rcpt in rcpt_tos {
            if !config.is_incoming_cleartext_ok(rcpt) {
                eprintln!("REJECT: Incoming unencrypted mail rejected for {}", rcpt);
                return Some(ENCRYPTION_NEEDED_523.to_string());
            }
        }
    }

    None
}

fn recipient_matches_passthrough(recipient: &str, passthrough_recipients: &[String]) -> bool {
    for addr in passthrough_recipients {
        if recipient == addr {
            return true;
        }
        if addr.starts_with('@') && recipient.ends_with(addr) {
            return true;
        }
    }
    false
}

async fn reinject(
    mail_from: &str,
    rcpt_tos: &[String],
    data: &[u8],
    port: u16,
) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).await?;
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    reader.read_line(&mut line).await?; // 220

    writer.write_all(format!("HELO localhost\r\n").as_bytes()).await?;
    line.clear();
    reader.read_line(&mut line).await?;

    writer.write_all(format!("MAIL FROM:<{}>\r\n", mail_from).as_bytes()).await?;
    line.clear();
    reader.read_line(&mut line).await?;

    for rcpt in rcpt_tos {
        writer.write_all(format!("RCPT TO:<{}>\r\n", rcpt).as_bytes()).await?;
        line.clear();
        reader.read_line(&mut line).await?;
    }

    writer.write_all(b"DATA\r\n").await?;
    line.clear();
    reader.read_line(&mut line).await?;

    // Escape dots
    let mut escaped_data = Vec::new();
    let data_str = std::str::from_utf8(data).unwrap_or("");
    for line in data_str.lines() {
        if line.starts_with('.') {
            escaped_data.extend_from_slice(b".");
        }
        escaped_data.extend_from_slice(line.as_bytes());
        escaped_data.extend_from_slice(b"\r\n");
    }
    
    writer.write_all(&escaped_data).await?;
    writer.write_all(b".\r\n").await?;
    line.clear();
    reader.read_line(&mut line).await?;

    writer.write_all(b"QUIT\r\n").await?;
    Ok(())
}
