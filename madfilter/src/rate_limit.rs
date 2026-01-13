use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct SendRateLimiter {
    addr2timestamps: Mutex<HashMap<String, Vec<Instant>>>,
}

impl SendRateLimiter {
    pub fn new() -> Self {
        Self {
            addr2timestamps: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_sending_allowed(&self, mail_from: &str, max_send_per_minute: u32) -> bool {
        let mut map = self.addr2timestamps.lock().unwrap();
        let timestamps = map.entry(mail_from.to_string()).or_insert_with(Vec::new);
        
        let now = Instant::now();
        let minute_ago = now - Duration::from_secs(60);
        
        // Remove old timestamps
        timestamps.retain(|&ts| ts >= minute_ago);
        
        if timestamps.len() as u32 <= max_send_per_minute {
            timestamps.push(now);
            true
        } else {
            false
        }
    }
}
