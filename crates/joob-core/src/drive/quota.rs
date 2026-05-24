use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::warn;

const DEFAULT_MAX_PER_MINUTE: u32 = 10_000; // conservative (Drive allows 12k)
const THROTTLE_THRESHOLD: f32 = 0.8; // start throttling at 80% of limit

pub struct QuotaTracker {
    state: Mutex<QuotaState>,
    max_per_minute: u32,
}

struct QuotaState {
    calls: VecDeque<Instant>,
    daily_count: u64,
    #[allow(dead_code)]
    day_start: Instant,
}

impl QuotaTracker {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QuotaState {
                calls: VecDeque::new(),
                daily_count: 0,
                day_start: Instant::now(),
            }),
            max_per_minute: DEFAULT_MAX_PER_MINUTE,
        }
    }

    pub fn with_max_per_minute(mut self, max: u32) -> Self {
        self.max_per_minute = max;
        self
    }

    /// Record an API call.
    pub fn record_call(&self) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.calls.push_back(now);
        state.daily_count += 1;

        // Prune old entries (older than 60 seconds)
        let cutoff = now - Duration::from_secs(60);
        while state.calls.front().map_or(false, |&t| t < cutoff) {
            state.calls.pop_front();
        }
    }

    /// Get current calls per minute.
    pub fn calls_per_minute(&self) -> u32 {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        // Prune old entries before counting
        let cutoff = now - Duration::from_secs(60);
        while state.calls.front().map_or(false, |&t| t < cutoff) {
            state.calls.pop_front();
        }

        state.calls.len() as u32
    }

    /// Check if we should throttle (approaching rate limit).
    pub fn should_throttle(&self) -> bool {
        let cpm = self.calls_per_minute();
        cpm >= (self.max_per_minute as f32 * THROTTLE_THRESHOLD) as u32
    }

    /// Get recommended delay before next API call.
    /// Returns `Duration::ZERO` if no throttling needed.
    pub fn recommended_delay(&self) -> Duration {
        if !self.should_throttle() {
            return Duration::ZERO;
        }

        let cpm = self.calls_per_minute();
        if cpm >= self.max_per_minute {
            // At limit — wait for oldest call to expire
            let state = self.state.lock().unwrap();
            if let Some(&oldest) = state.calls.front() {
                let age = oldest.elapsed();
                if age < Duration::from_secs(60) {
                    return Duration::from_secs(60) - age;
                }
            }
            Duration::from_millis(100)
        } else {
            // Approaching limit — small delay
            Duration::from_millis(50)
        }
    }

    /// Get daily call count.
    pub fn daily_count(&self) -> u64 {
        self.state.lock().unwrap().daily_count
    }

    /// Apply throttling: sleep if needed before making an API call.
    pub async fn throttle(&self) {
        let delay = self.recommended_delay();
        if !delay.is_zero() {
            warn!(delay_ms = delay.as_millis(), cpm = self.calls_per_minute(), "throttling");
            tokio::time::sleep(delay).await;
        }
        self.record_call();
    }
}

impl Default for QuotaTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let qt = QuotaTracker::new();
        assert_eq!(qt.calls_per_minute(), 0);
        assert!(!qt.should_throttle());
        assert_eq!(qt.daily_count(), 0);
    }

    #[test]
    fn test_record_calls() {
        let qt = QuotaTracker::new();
        for _ in 0..100 {
            qt.record_call();
        }
        assert_eq!(qt.calls_per_minute(), 100);
        assert_eq!(qt.daily_count(), 100);
    }

    #[test]
    fn test_throttle_threshold() {
        let qt = QuotaTracker::new().with_max_per_minute(100);
        for _ in 0..79 {
            qt.record_call();
        }
        assert!(!qt.should_throttle());

        qt.record_call(); // 80th call = 80% of 100
        assert!(qt.should_throttle());
    }

    #[test]
    fn test_recommended_delay_no_throttle() {
        let qt = QuotaTracker::new();
        assert_eq!(qt.recommended_delay(), Duration::ZERO);
    }
}
