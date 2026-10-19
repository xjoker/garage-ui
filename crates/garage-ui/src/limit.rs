use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use governor::clock::{Clock, DefaultClock};
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use tokio::sync::Semaphore;

type Limiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

/// Default number of login attempts one client address may make per minute, across all
/// usernames. `LOGIN_IP_LIMIT_PER_MINUTE` overrides it.
pub const DEFAULT_LOGIN_IP_PER_MINUTE: NonZeroU32 = NonZeroU32::new(30).expect("30");

/// Unauthenticated audit rows that a client address can cause without an account to bind
/// them to (illegal usernames, 429 responses) are written up to this many per minute.
/// Further rows are counted and written later as one `audit_suppressed` summary row.
const NOISY_AUDIT_PER_MINUTE: NonZeroU32 = NonZeroU32::new(10).expect("10");

/// Upper bound on distinct (address, path) suppression counters held between flushes.
/// Overflow is folded into one counter per path so memory stays bounded.
const MAX_SUPPRESSED_KEYS: usize = 4096;

/// Address recorded for counters folded together after `MAX_SUPPRESSED_KEYS`.
pub const OVERFLOW_ADDRESS: &str = "*";

/// One pending `audit_suppressed` summary: the client address key, the request path and
/// the number of rows that were not written.
#[derive(Debug, PartialEq, Eq)]
pub struct Suppressed {
    pub ip: String,
    pub path: &'static str,
    pub count: u64,
}

pub struct Limits {
    login: Limiter,
    login_ip: Limiter,
    init: Limiter,
    password: Limiter,
    share: Limiter,
    noisy_audit: Limiter,
    suppressed: Mutex<HashMap<(String, &'static str), u64>>,
    clock: DefaultClock,
}

impl Limits {
    pub fn new(quota: Quota) -> Self {
        Self::with_login_ip_quota(quota, Quota::per_minute(DEFAULT_LOGIN_IP_PER_MINUTE))
    }

    pub fn with_login_ip_quota(quota: Quota, login_ip: Quota) -> Self {
        Self {
            login: RateLimiter::keyed(quota),
            login_ip: RateLimiter::keyed(login_ip),
            init: RateLimiter::keyed(quota),
            password: RateLimiter::keyed(quota),
            share: RateLimiter::keyed(quota),
            noisy_audit: RateLimiter::keyed(Quota::per_minute(NOISY_AUDIT_PER_MINUTE)),
            suppressed: Mutex::new(HashMap::new()),
            clock: DefaultClock::default(),
        }
    }

    pub fn login(&self, ip: &str, username: &str) -> Result<(), Duration> {
        let key = format!("{ip}\n{}", username.to_lowercase());
        self.hit(&self.login, &key)
    }

    /// Total login attempts from one client address, whatever the username. Every login
    /// request passes here first, including ones whose username can never be valid.
    pub fn login_address(&self, ip: &str) -> Result<(), Duration> {
        self.hit(&self.login_ip, &address_key(ip))
    }

    /// Returns true when an unauthenticated, account-less audit row from this address
    /// should be written now. Otherwise the row is counted for a later summary.
    pub fn admit_noisy_audit(&self, ip: &str, path: &'static str) -> bool {
        let key = address_key(ip);
        if self.hit(&self.noisy_audit, &key).is_ok() {
            return true;
        }
        let mut suppressed = self.suppressed.lock().expect("suppressed audit counters");
        let slot = if suppressed.contains_key(&(key.clone(), path))
            || suppressed.len() < MAX_SUPPRESSED_KEYS
        {
            (key, path)
        } else {
            (OVERFLOW_ADDRESS.to_string(), path)
        };
        *suppressed.entry(slot).or_insert(0) += 1;
        false
    }

    /// Drains the suppressed-row counters, so the caller can write one summary row each.
    pub fn take_suppressed(&self) -> Vec<Suppressed> {
        let drained =
            std::mem::take(&mut *self.suppressed.lock().expect("suppressed audit counters"));
        let mut out: Vec<Suppressed> = drained
            .into_iter()
            .map(|((ip, path), count)| Suppressed { ip, path, count })
            .collect();
        out.sort_by(|a, b| (a.ip.as_str(), a.path).cmp(&(b.ip.as_str(), b.path)));
        out
    }

    /// Puts counters back after a failed write so the next flush retries them.
    pub fn restore_suppressed(&self, rows: Vec<Suppressed>) {
        let mut suppressed = self.suppressed.lock().expect("suppressed audit counters");
        for row in rows {
            let slot = if suppressed.contains_key(&(row.ip.clone(), row.path))
                || suppressed.len() < MAX_SUPPRESSED_KEYS
            {
                (row.ip, row.path)
            } else {
                (OVERFLOW_ADDRESS.to_string(), row.path)
            };
            *suppressed.entry(slot).or_insert(0) += row.count;
        }
    }

    pub fn init(&self, ip: &str) -> Result<(), Duration> {
        self.hit(&self.init, ip)
    }

    pub fn password(&self, username: &str) -> Result<(), Duration> {
        self.hit(&self.password, &username.to_lowercase())
    }

    pub fn share(&self, username: &str) -> Result<(), Duration> {
        self.hit(&self.share, &username.to_lowercase())
    }

    pub fn retain_recent(&self) {
        for limiter in [
            &self.login,
            &self.login_ip,
            &self.init,
            &self.password,
            &self.share,
            &self.noisy_audit,
        ] {
            limiter.retain_recent();
            limiter.shrink_to_fit();
        }
    }

    #[cfg(test)]
    pub fn login_len(&self) -> usize {
        self.login.len()
    }

    #[cfg(test)]
    pub fn login_ip_len(&self) -> usize {
        self.login_ip.len()
    }

    #[cfg(test)]
    pub fn noisy_audit_len(&self) -> usize {
        self.noisy_audit.len()
    }

    #[cfg(test)]
    pub fn share_len(&self) -> usize {
        self.share.len()
    }

    fn hit(&self, limiter: &Limiter, key: &str) -> Result<(), Duration> {
        match limiter.check_key(&key.to_string()) {
            Ok(_) => Ok(()),
            Err(not_until) => Err(not_until.wait_time_from(self.clock.now())),
        }
    }
}

/// Limiter key for one client address. The address comes from `session::client_ip`, so it
/// already follows `TRUSTED_PROXY`. IPv6 clients are grouped by their /64 network, because
/// one host usually controls a whole /64. IPv4-mapped IPv6 addresses count as IPv4.
/// Anything that does not parse (a misconfigured proxy header) is cut to 64 bytes so one
/// key cannot hold an arbitrarily large string.
pub fn address_key(ip: &str) -> String {
    match ip.trim().parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.to_string(),
        Ok(IpAddr::V6(v6)) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let mut segments = v6.segments();
                for segment in &mut segments[4..] {
                    *segment = 0;
                }
                format!("{}/64", std::net::Ipv6Addr::from(segments))
            }
        },
        Err(_) => {
            let mut end = ip.len().min(64);
            while !ip.is_char_boundary(end) {
                end -= 1;
            }
            ip[..end].to_string()
        }
    }
}

pub fn production_quota() -> Quota {
    Quota::per_minute(NonZeroU32::new(10).expect("10"))
}

pub const DEFAULT_FANOUT_IN_FLIGHT: NonZeroUsize = NonZeroUsize::new(8).expect("8");

pub async fn fan_out<T, R, F, Fut>(limit: NonZeroUsize, items: Vec<T>, f: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    let permits = Arc::new(Semaphore::new(limit.get()));
    let f = Arc::new(f);
    let mut joins = Vec::with_capacity(items.len());
    for item in items {
        let permits = Arc::clone(&permits);
        let f = Arc::clone(&f);
        joins.push(tokio::spawn(async move {
            let _permit = permits
                .acquire_owned()
                .await
                .expect("fan-out semaphore stays open");
            f(item).await
        }));
    }
    let mut out = Vec::with_capacity(joins.len());
    for join in joins {
        out.push(join.await.expect("fan-out task"));
    }
    out
}

#[cfg(test)]
pub fn test_quota() -> Quota {
    Quota::with_period(Duration::from_millis(200))
        .expect("period")
        .allow_burst(NonZeroU32::new(10).expect("10"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Instant;

    use super::*;

    #[test]
    fn production_quota_allows_ten_then_blocks() {
        let limits = Limits::new(production_quota());
        for _ in 0..10 {
            assert!(limits.login("127.0.0.1", "Ada").is_ok());
        }
        assert!(limits.login("127.0.0.1", "ada").is_err());
        assert!(limits.login("127.0.0.1", "other").is_ok());
    }

    #[test]
    fn login_limit_gives_each_client_address_its_own_bucket() {
        let limits = Limits::new(production_quota());
        for _ in 0..10 {
            assert!(limits.login("10.0.0.1", "Ada").is_ok());
        }
        assert!(limits.login("10.0.0.1", "ada").is_err());
        assert!(limits.login("10.0.0.2", "Ada").is_ok());
        for _ in 0..9 {
            assert!(limits.login("10.0.0.2", "ADA").is_ok());
        }
        assert!(limits.login("10.0.0.2", "ada").is_err());
        assert!(limits.login("10.0.0.3", "Ada").is_ok());
    }

    #[test]
    fn login_address_budget_spans_usernames_and_addresses_are_independent() {
        let limits = Limits::with_login_ip_quota(
            production_quota(),
            Quota::per_minute(NonZeroU32::new(3).expect("3")),
        );
        for _ in 0..3 {
            assert!(limits.login_address("10.0.0.1").is_ok());
        }
        assert!(limits.login_address("10.0.0.1").is_err());
        assert!(limits.login_address("10.0.0.2").is_ok());
    }

    #[test]
    fn default_login_address_budget_is_thirty_per_minute() {
        let limits = Limits::new(production_quota());
        for _ in 0..30 {
            assert!(limits.login_address("10.0.0.1").is_ok());
        }
        let wait = limits.login_address("10.0.0.1").unwrap_err();
        assert!(wait <= Duration::from_secs(2), "{wait:?}");
    }

    #[test]
    fn address_key_groups_ipv6_by_network_and_bounds_garbage() {
        assert_eq!(address_key("192.0.2.7"), "192.0.2.7");
        assert_eq!(address_key("::ffff:192.0.2.7"), "192.0.2.7");
        assert_eq!(address_key("2001:db8:1:2:aaaa::1"), "2001:db8:1:2::/64");
        assert_eq!(
            address_key("2001:db8:1:2:ffff:ffff:ffff:ffff"),
            address_key("2001:db8:1:2::9")
        );
        assert_ne!(
            address_key("2001:db8:1:3::1"),
            address_key("2001:db8:1:2::1")
        );
        assert_eq!(address_key(&"a".repeat(10_000)).len(), 64);
        assert_eq!(address_key(&"é".repeat(100)).len(), 64);
    }

    #[test]
    fn noisy_audit_admits_ten_per_address_then_counts() {
        let limits = Limits::new(production_quota());
        let path = "/api/auth/login";
        let admitted = (0..25)
            .filter(|_| limits.admit_noisy_audit("10.0.0.1", path))
            .count();
        assert_eq!(admitted, 10);
        assert!(limits.admit_noisy_audit("10.0.0.2", path));
        assert!(!limits.admit_noisy_audit("10.0.0.1", "/api/setup"));
        assert_eq!(
            limits.take_suppressed(),
            vec![
                Suppressed {
                    ip: "10.0.0.1".into(),
                    path: "/api/auth/login",
                    count: 15
                },
                Suppressed {
                    ip: "10.0.0.1".into(),
                    path: "/api/setup",
                    count: 1
                },
            ]
        );
        assert!(
            limits.take_suppressed().is_empty(),
            "take drains the counters"
        );
    }

    #[test]
    fn suppressed_counters_are_capped() {
        let limits = Limits::new(production_quota());
        let path = "/api/auth/login";
        for index in 0..(MAX_SUPPRESSED_KEYS + 50) {
            let ip = format!(
                "10.{}.{}.{}",
                index >> 16 & 255,
                index >> 8 & 255,
                index & 255
            );
            for _ in 0..11 {
                limits.admit_noisy_audit(&ip, path);
            }
        }
        let rows = limits.take_suppressed();
        assert_eq!(rows.len(), MAX_SUPPRESSED_KEYS + 1);
        let overflow = rows
            .iter()
            .find(|row| row.ip == OVERFLOW_ADDRESS)
            .expect("overflow row");
        assert_eq!(overflow.count, 50);
        assert_eq!(
            rows.iter().map(|row| row.count).sum::<u64>(),
            (MAX_SUPPRESSED_KEYS + 50) as u64
        );
    }

    #[tokio::test]
    async fn retain_recent_drops_keys_whose_quota_has_replenished() {
        let limits = Limits::with_login_ip_quota(test_quota(), test_quota());
        for user in ["a", "b", "c"] {
            let _ = limits.login("10.0.0.1", user);
            let _ = limits.share(user);
            let _ = limits.login_address(user);
            let _ = limits.admit_noisy_audit(user, "/api/auth/login");
        }
        assert_eq!(limits.login_len(), 3);
        assert_eq!(limits.login_ip_len(), 3);
        assert_eq!(limits.noisy_audit_len(), 3);
        limits.retain_recent();
        assert_eq!(limits.login_len(), 3, "fresh keys are kept");
        // Ten burst slots replenish after 10 * 200 ms.
        tokio::time::sleep(Duration::from_millis(2_300)).await;
        limits.retain_recent();
        assert_eq!(limits.login_len(), 0);
        assert_eq!(limits.login_ip_len(), 0);
        assert_eq!(limits.share_len(), 0);
    }

    #[tokio::test]
    async fn one_thousand_calls_stay_at_or_under_eight() {
        let (high, got) = wave(DEFAULT_FANOUT_IN_FLIGHT, 1000).await;
        assert!(high <= 8, "high water {high}");
        assert_eq!(got, (0..1000).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn limit_two_stays_at_or_under_two() {
        let limit = NonZeroUsize::new(2).expect("2");
        let (high, got) = wave(limit, 1000).await;
        assert!(high <= 2, "high water {high}");
        assert_eq!(got, (0..1000).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn limit_one_runs_calls_one_at_a_time() {
        let limit = NonZeroUsize::new(1).expect("1");
        let (high, got) = wave(limit, 1000).await;
        assert_eq!(high, 1);
        assert_eq!(got, (0..1000).collect::<Vec<_>>());
    }

    async fn wave(limit: NonZeroUsize, count: usize) -> (usize, Vec<usize>) {
        let inflight = Arc::new(AtomicUsize::new(0));
        let high = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(AtomicBool::new(false));
        let inflight_task = Arc::clone(&inflight);
        let high_task = Arc::clone(&high);
        let release_task = Arc::clone(&release);
        let driver = tokio::spawn(async move {
            fan_out(limit, (0..count).collect::<Vec<usize>>(), move |item| {
                let inflight = Arc::clone(&inflight_task);
                let high = Arc::clone(&high_task);
                let release = Arc::clone(&release_task);
                async move {
                    let now = inflight.fetch_add(1, Ordering::SeqCst) + 1;
                    high.fetch_max(now, Ordering::SeqCst);
                    while !release.load(Ordering::SeqCst) {
                        tokio::task::yield_now().await;
                    }
                    inflight.fetch_sub(1, Ordering::SeqCst);
                    let skew = limit.get() - 1 - (item % limit.get());
                    for _ in 0..skew {
                        tokio::task::yield_now().await;
                    }
                    item
                }
            })
            .await
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let now = inflight.load(Ordering::SeqCst);
            assert!(
                now <= limit.get(),
                "in flight {now} exceeds {}",
                limit.get()
            );
            if now == limit.get() {
                tokio::task::yield_now().await;
                let again = inflight.load(Ordering::SeqCst);
                assert!(
                    again <= limit.get(),
                    "in flight {again} exceeds {}",
                    limit.get()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "in flight stayed at {now}, limit {}",
                limit.get()
            );
            tokio::task::yield_now().await;
        }
        release.store(true, Ordering::SeqCst);
        let got = driver.await.expect("driver");
        (high.load(Ordering::SeqCst), got)
    }
}
