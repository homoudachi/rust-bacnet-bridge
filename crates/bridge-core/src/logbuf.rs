use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: &str) -> LogEntry {
        LogEntry {
            timestamp: "2025-01-01T00:00:00Z".into(),
            level: level.into(),
            target: "test".into(),
            message: "test message".into(),
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
    fn test_level_value() {
        assert_eq!(super::level_value("TRACE"), 0);
        assert_eq!(super::level_value("DEBUG"), 1);
        assert_eq!(super::level_value("INFO"), 2);
        assert_eq!(super::level_value("WARN"), 3);
        assert_eq!(super::level_value("ERROR"), 4);
        assert_eq!(super::level_value("UNKNOWN"), 5);
    }
}
