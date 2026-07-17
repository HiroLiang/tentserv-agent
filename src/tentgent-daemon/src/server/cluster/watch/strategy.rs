use std::time::Instant;

use super::domain::DefinitionWatchSchedule;

pub(super) struct HybridWatchStrategy {
    schedule: DefinitionWatchSchedule,
    last_strong_hash: Instant,
}

impl HybridWatchStrategy {
    pub(super) fn new_at(schedule: DefinitionWatchSchedule, now: Instant) -> Self {
        Self {
            schedule,
            last_strong_hash: now,
        }
    }

    pub(super) fn should_force_hash(&mut self, now: Instant) -> bool {
        if now.duration_since(self.last_strong_hash) < self.schedule.strong_hash_interval {
            return false;
        }
        self.last_strong_hash = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn strong_hash_is_periodic_without_real_sleep() {
        let started = Instant::now();
        let schedule = DefinitionWatchSchedule {
            metadata_interval: Duration::from_secs(1),
            strong_hash_interval: Duration::from_secs(30),
        };
        let mut strategy = HybridWatchStrategy::new_at(schedule, started);
        assert!(!strategy.should_force_hash(started + Duration::from_secs(29)));
        assert!(strategy.should_force_hash(started + Duration::from_secs(30)));
        assert!(!strategy.should_force_hash(started + Duration::from_secs(59)));
        assert!(strategy.should_force_hash(started + Duration::from_secs(60)));
    }
}
