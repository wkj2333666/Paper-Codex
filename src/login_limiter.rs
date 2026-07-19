use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct LoginLimitPolicy {
    pub max_failures: u32,
    pub window: Duration,
    pub block: Duration,
    pub max_clients: usize,
}

impl Default for LoginLimitPolicy {
    fn default() -> Self {
        Self {
            max_failures: 5,
            window: Duration::from_secs(10 * 60),
            block: Duration::from_secs(15 * 60),
            max_clients: 4096,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoginLimiter {
    policy: LoginLimitPolicy,
    clients: Arc<Mutex<HashMap<IpAddr, ClientAttempts>>>,
}

#[derive(Debug, Clone, Copy)]
struct ClientAttempts {
    window_started: Instant,
    failures: u32,
    blocked_until: Option<Instant>,
    last_seen: Instant,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new(LoginLimitPolicy::default())
    }
}

impl LoginLimiter {
    pub fn new(policy: LoginLimitPolicy) -> Self {
        assert!(policy.max_failures > 0, "max_failures must be positive");
        assert!(policy.max_clients > 0, "max_clients must be positive");
        Self {
            policy,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn check_at(&self, client: IpAddr, now: Instant) -> Option<Duration> {
        let mut clients = self.clients.lock().expect("login limiter mutex poisoned");
        self.prune(&mut clients, now);
        let attempts = clients.get_mut(&client)?;
        attempts.last_seen = now;
        attempts
            .blocked_until
            .filter(|until| *until > now)
            .map(|until| until.saturating_duration_since(now))
    }

    pub fn record_failure_at(&self, client: IpAddr, now: Instant) {
        let mut clients = self.clients.lock().expect("login limiter mutex poisoned");
        self.prune(&mut clients, now);
        if !clients.contains_key(&client) && clients.len() >= self.policy.max_clients {
            if let Some(oldest) = clients
                .iter()
                .min_by_key(|(_, attempts)| attempts.last_seen)
                .map(|(ip, _)| *ip)
            {
                clients.remove(&oldest);
            }
        }
        let attempts = clients.entry(client).or_insert(ClientAttempts {
            window_started: now,
            failures: 0,
            blocked_until: None,
            last_seen: now,
        });
        attempts.failures += 1;
        attempts.last_seen = now;
        if attempts.failures >= self.policy.max_failures {
            attempts.blocked_until = Some(now + self.policy.block);
        }
    }

    pub fn clear(&self, client: IpAddr) {
        self.clients
            .lock()
            .expect("login limiter mutex poisoned")
            .remove(&client);
    }

    pub fn tracked_clients(&self) -> usize {
        self.clients
            .lock()
            .expect("login limiter mutex poisoned")
            .len()
    }

    fn prune(&self, clients: &mut HashMap<IpAddr, ClientAttempts>, now: Instant) {
        clients.retain(|_, attempts| {
            if let Some(blocked_until) = attempts.blocked_until {
                blocked_until > now
            } else {
                now.saturating_duration_since(attempts.window_started) < self.policy.window
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{LoginLimitPolicy, LoginLimiter};
    use std::{
        net::IpAddr,
        str::FromStr,
        time::{Duration, Instant},
    };

    fn ip(value: &str) -> IpAddr {
        IpAddr::from_str(value).unwrap()
    }

    fn policy(max_clients: usize) -> LoginLimitPolicy {
        LoginLimitPolicy {
            max_failures: 5,
            window: Duration::from_secs(10 * 60),
            block: Duration::from_secs(15 * 60),
            max_clients,
        }
    }

    #[test]
    fn fifth_failure_blocks_for_fifteen_minutes() {
        let limiter = LoginLimiter::new(policy(4096));
        let client = ip("198.51.100.10");
        let now = Instant::now();

        for _ in 0..5 {
            assert_eq!(limiter.check_at(client, now), None);
            limiter.record_failure_at(client, now);
        }

        assert_eq!(
            limiter.check_at(client, now),
            Some(Duration::from_secs(900))
        );
        assert_eq!(
            limiter.check_at(client, now + Duration::from_secs(899)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            limiter.check_at(client, now + Duration::from_secs(900)),
            None
        );
    }

    #[test]
    fn failures_expire_after_the_ten_minute_window() {
        let limiter = LoginLimiter::new(policy(4096));
        let client = ip("198.51.100.10");
        let now = Instant::now();
        for _ in 0..4 {
            limiter.record_failure_at(client, now);
        }

        assert_eq!(
            limiter.check_at(client, now + Duration::from_secs(600)),
            None
        );
        for _ in 0..4 {
            limiter.record_failure_at(client, now + Duration::from_secs(600));
        }
        assert_eq!(
            limiter.check_at(client, now + Duration::from_secs(600)),
            None
        );
    }

    #[test]
    fn successful_login_clears_failures_and_clients_are_isolated() {
        let limiter = LoginLimiter::new(policy(4096));
        let first = ip("198.51.100.10");
        let second = ip("198.51.100.11");
        let now = Instant::now();
        for _ in 0..5 {
            limiter.record_failure_at(first, now);
        }

        assert!(limiter.check_at(first, now).is_some());
        assert_eq!(limiter.check_at(second, now), None);
        limiter.clear(first);
        assert_eq!(limiter.check_at(first, now), None);
    }

    #[test]
    fn client_table_evicts_the_oldest_record_at_capacity() {
        let limiter = LoginLimiter::new(policy(2));
        let now = Instant::now();
        let first = ip("198.51.100.10");
        let second = ip("198.51.100.11");
        let third = ip("198.51.100.12");

        limiter.record_failure_at(first, now);
        limiter.record_failure_at(second, now + Duration::from_secs(1));
        limiter.record_failure_at(third, now + Duration::from_secs(2));

        assert_eq!(limiter.tracked_clients(), 2);
        assert_eq!(limiter.check_at(first, now + Duration::from_secs(2)), None);
    }
}
