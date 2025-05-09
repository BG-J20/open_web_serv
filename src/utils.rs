use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use libc::{time_t, tm};
use sha2::{Digest, Sha256};
use urlencoding::decode;

extern "C" {
    fn localtime(time: *const time_t) -> *mut tm;
}

pub fn log_to_file(message: &str) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("log.txt")?;
    file.write_all(message.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn get_formatted_time() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let t: time_t = now as time_t;

    unsafe {
        let tm_ptr: *mut tm = localtime(&t);
        if tm_ptr.is_null() {
            return "[время неизвестно]".to_string();
        }
        let tm = *tm_ptr;
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

pub fn parse_form_data(body: &str) -> HashMap<String, String> {
    let mut data = HashMap::new();
    for pair in body.split('&') {
        let mut split = pair.splitn(2, '=');
        if let (Some(k), Some(v)) = (split.next(), split.next()) {
            let key = decode(k).unwrap_or_default().to_string();
            let value = decode(v).unwrap_or_default().to_string();
            data.insert(key, value);
        }
    }
    data
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}