use serde::Serialize;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, serde_json::Value>,
}

pub struct LogRingBuffer {
    entries: Mutex<Vec<LogEntry>>,
    capacity: usize,
}

fn level_value(level: &str) -> u8 {
    match level {
        "TRACE" => 0,
        "DEBUG" => 1,
        "INFO" => 2,
        "WARN" => 3,
        "ERROR" => 4,
        _ => 5,
    }
}

impl LogRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn push(&self, entry: LogEntry) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.capacity {
            entries.remove(0);
        }
        entries.push(entry);
    }

    pub fn recent(&self, limit: usize, min_level: Option<&str>) -> Vec<LogEntry> {
        let entries = self.entries.lock().unwrap();
        let min_val = min_level.map(level_value).unwrap_or(0);
        let filtered: Vec<_> = entries
            .iter()
            .filter(|e| level_value(&e.level) >= min_val)
            .cloned()
            .collect();
        let len = filtered.len();
        let start = len.saturating_sub(limit);
        filtered.into_iter().skip(start).collect()
    }
}

/// A tracing-compatible writer that pushes formatted log lines into a LogRingBuffer.
pub struct LogBufWriter {
    buf: Arc<LogRingBuffer>,
}

impl LogBufWriter {
    pub fn new(buf: Arc<LogRingBuffer>) -> Self {
        Self { buf }
    }
}

impl io::Write for LogBufWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(data);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry = parse_tracing_line(line);
            self.buf.push(entry);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && !(bytes[i] as char).is_ascii_alphabetic() {
                i += 1;
            }
            if i < bytes.len() { i += 1; }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn parse_tracing_line(line: &str) -> LogEntry {
    let line = strip_ansi(line);
    let mut parts = line.splitn(2, ' ');
    let timestamp = parts.next().unwrap_or("").to_string();
    let rest = parts.next().unwrap_or("");

    let rest = rest.trim_start();
    let mut parts = rest.splitn(2, ' ');
    let level = parts.next().unwrap_or("INFO").to_string();
    let target_msg = parts.next().unwrap_or("");

    let (target, message) = if let Some(pos) = target_msg.find(": ") {
        (
            target_msg[..pos].to_string(),
            target_msg[pos + 2..].to_string(),
        )
    } else {
        (String::new(), target_msg.to_string())
    };

    LogEntry {
        timestamp,
        level,
        target,
        message,
        fields: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: &str) -> LogEntry {
        LogEntry {
            timestamp: "2025-01-01T00:00:00Z".into(),
            level: level.into(),
            target: "test".into(),
            message: "test message".into(),
            fields: HashMap::new(),
        }
    }

    #[test]
    fn test_push_and_recent() {
        let buf = LogRingBuffer::new(100);
        buf.push(entry("INFO"));
        buf.push(entry("DEBUG"));
        assert_eq!(buf.recent(10, None).len(), 2);
    }

    #[test]
    fn test_capacity_drop_oldest() {
        let buf = LogRingBuffer::new(3);
        buf.push(entry("INFO"));
        buf.push(entry("INFO"));
        buf.push(entry("INFO"));
        buf.push(entry("ERROR"));
        let recent = buf.recent(10, None);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].level, "INFO");
        assert_eq!(recent[2].level, "ERROR");
    }

    #[test]
    fn test_recent_limit() {
        let buf = LogRingBuffer::new(100);
        for _ in 0..20 {
            buf.push(entry("INFO"));
        }
        assert_eq!(buf.recent(5, None).len(), 5);
    }

    #[test]
    fn test_filter_by_level() {
        let buf = LogRingBuffer::new(100);
        buf.push(entry("DEBUG"));
        buf.push(entry("INFO"));
        buf.push(entry("WARN"));
        buf.push(entry("ERROR"));
        let warn_up = buf.recent(10, Some("WARN"));
        assert_eq!(warn_up.len(), 2);
        for e in &warn_up {
            assert!(e.level == "WARN" || e.level == "ERROR");
        }
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[2m2025-01-01T00:00:00Z\x1b[0m \x1b[31mERROR\x1b[0m \x1b[1mmy_target\x1b[0m: \x1b[36msomething happened\x1b[0m";
        let entry = parse_tracing_line(input);
        assert!(!entry.timestamp.contains('\x1b'));
        assert_eq!(entry.level, "ERROR");
        assert!(!entry.target.contains('\x1b'));
        assert!(!entry.message.contains('\x1b'));
        assert_eq!(entry.message, "something happened");
    }

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi(""), "");
        assert_eq!(strip_ansi("plain text"), "plain text");
        assert_eq!(strip_ansi("\x1b[1m\x1b[0m"), "");
    }

    #[test]
    fn test_level_value() {
        assert_eq!(super::level_value("TRACE"), 0);
        assert_eq!(super::level_value("DEBUG"), 1);
        assert_eq!(super::level_value("INFO"), 2);
        assert_eq!(super::level_value("WARN"), 3);
        assert_eq!(super::level_value("ERROR"), 4);
        assert_eq!(super::level_value("UNKNOWN"), 5);
    }
}
