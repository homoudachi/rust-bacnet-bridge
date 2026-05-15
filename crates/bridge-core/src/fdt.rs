use serde::Serialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bacnet_transport::bbmd::FdtEntry;

fn epoch_to_iso(secs: u64, millis: u32) -> String {
    // Simple ISO 8601 formatting without external deps
    const SECS_PER_DAY: u64 = 86400;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_MIN: u64 = 60;

    let days = secs / SECS_PER_DAY;
    let rem = secs % SECS_PER_DAY;
    let hours = rem / SECS_PER_HOUR;
    let rem = rem % SECS_PER_HOUR;
    let minutes = rem / SECS_PER_MIN;
    let seconds = rem % SECS_PER_MIN;

    // Days since epoch to year/month/day (civil from UNIX epoch)
    let days_since_epoch = days as i64;
    let mut y = 1970i64;
    let mut d = days_since_epoch;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if d < year_days {
            break;
        }
        d -= year_days;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1usize;
    for &md in &month_days {
        if d < md {
            break;
        }
        d -= md;
        m += 1;
    }
    let day = d + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, day, hours, minutes, seconds, millis
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[derive(Debug, Clone, Serialize)]
pub struct FdtDisplayEntry {
    pub ip: String,
    pub port: u16,
    pub ttl: u16,
    pub remaining_ttl: i64,
    pub registered_at: String,
}

#[derive(Debug)]
pub struct FdtManager {
    entries: Vec<FdtEntry>,
}

impl FdtManager {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn list(&self) -> Vec<FdtDisplayEntry> {
        self.entries
            .iter()
            .map(|entry| {
                let ip = format!(
                    "{}.{}.{}.{}",
                    entry.ip[0], entry.ip[1], entry.ip[2], entry.ip[3]
                );

                let elapsed = entry.registered_at.elapsed().as_secs() as i64;
                let remaining_ttl = entry.ttl as i64 - elapsed;

                let registered_at = {
                    let since_epoch = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::ZERO);
                    let reg_epoch = since_epoch
                        .checked_sub(entry.registered_at.elapsed())
                        .unwrap_or(Duration::ZERO);
                    let secs = reg_epoch.as_secs();
                    let millis = reg_epoch.subsec_millis();
                    epoch_to_iso(secs, millis)
                };

                FdtDisplayEntry {
                    ip,
                    port: entry.port,
                    ttl: entry.ttl,
                    remaining_ttl,
                    registered_at,
                }
            })
            .collect()
    }

    pub fn tick(&mut self) {
        self.entries
            .retain(|e| (e.ttl as u64) + GRACE_PERIOD_SECS >= e.registered_at.elapsed().as_secs());
    }

    pub fn add(&mut self, ip: [u8; 4], port: u16, ttl: u16) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.ip == ip && e.port == port)
        {
            existing.ttl = ttl;
            existing.registered_at = Instant::now();
        } else {
            self.entries.push(FdtEntry {
                ip,
                port,
                ttl,
                registered_at: Instant::now(),
            });
        }
    }

    pub fn remove(&mut self, ip: [u8; 4], port: u16) {
        self.entries.retain(|e| !(e.ip == ip && e.port == port));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FdtManager {
    fn default() -> Self {
        Self::new()
    }
}

const GRACE_PERIOD_SECS: u64 = 30;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_entry(ip: [u8; 4], port: u16, ttl: u16, age_secs: u64) -> FdtEntry {
        FdtEntry {
            ip,
            port,
            ttl,
            registered_at: Instant::now() - Duration::from_secs(age_secs),
        }
    }

    #[test]
    fn test_add_and_list() {
        let mut mgr = FdtManager::new();
        assert!(mgr.is_empty());

        mgr.add([10, 0, 0, 5], 47808, 60);
        assert_eq!(mgr.len(), 1);

        let list = mgr.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].ip, "10.0.0.5");
        assert_eq!(list[0].port, 47808);
        assert_eq!(list[0].ttl, 60);
    }

    #[test]
    fn test_remove_entry() {
        let mut mgr = FdtManager::new();
        mgr.add([10, 0, 0, 5], 47808, 60);
        mgr.add([10, 0, 0, 6], 47808, 120);
        assert_eq!(mgr.len(), 2);

        mgr.remove([10, 0, 0, 5], 47808);
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.list()[0].ip, "10.0.0.6");
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut mgr = FdtManager::new();
        mgr.add([10, 0, 0, 5], 47808, 60);
        mgr.remove([10, 0, 0, 99], 47808);
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn test_re_add_updates_ttl() {
        let mut mgr = FdtManager::new();
        mgr.add([10, 0, 0, 5], 47808, 60);
        mgr.add([10, 0, 0, 5], 47808, 120);
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.list()[0].ttl, 120);
    }

    #[test]
    fn test_ttl_expiry_purge() {
        let mut mgr = FdtManager::new();
        // Manually insert an entry past TTL + grace period
        mgr.entries.push(make_entry([10, 0, 0, 5], 47808, 10, 50)); // 10s TTL + 30s grace = 40s, age=50s
        mgr.tick();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_within_grace_period_not_purged() {
        let mut mgr = FdtManager::new();
        // TTL=10s, grace=30s, total=40s, age=35s → within grace
        mgr.entries.push(make_entry([10, 0, 0, 5], 47808, 10, 35));
        mgr.tick();
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn test_purge_only_expired() {
        let mut mgr = FdtManager::new();
        mgr.entries.push(make_entry([10, 0, 0, 5], 47808, 10, 50)); // expired
        mgr.entries.push(make_entry([10, 0, 0, 6], 47808, 60, 5)); // fresh
        mgr.entries.push(make_entry([10, 0, 0, 7], 47808, 30, 25)); // within grace (30+30=60, age=25)
        mgr.tick();
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn test_tick_noop_when_no_expired() {
        let mut mgr = FdtManager::new();
        mgr.add([10, 0, 0, 5], 47808, 300);
        mgr.add([10, 0, 0, 6], 47808, 600);
        mgr.tick();
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn test_remaining_ttl_computation() {
        let mut mgr = FdtManager::new();
        mgr.add([10, 0, 0, 5], 47808, 60);
        let list = mgr.list();
        assert_eq!(list.len(), 1);
        // remaining_ttl should be close to 60 (just registered)
        assert!(list[0].remaining_ttl > 55 && list[0].remaining_ttl <= 60);
    }

    #[test]
    fn test_display_entry_has_timestamp() {
        let mut mgr = FdtManager::new();
        mgr.add([10, 0, 0, 5], 47808, 60);
        let list = mgr.list();
        assert!(!list[0].registered_at.is_empty());
        // Should look like ISO date
        assert!(list[0].registered_at.contains('T'));
        assert!(list[0].registered_at.ends_with('Z'));
    }
}
