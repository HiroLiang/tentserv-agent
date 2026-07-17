use std::{thread, time::Instant};

use sha2::{Digest, Sha256};

use crate::{
    features::resource_coordination::{
        new_operation_id, ResourceBusy, ResourceCoordinationCode, ResourceKey,
    },
    foundation::error::KernelResult,
};

const STABILIZATION_ATTEMPTS: u32 = 3;

pub enum ResourceTransitionAuthorization<Permit, Rejection> {
    Permitted(Permit),
    Rejected(Rejection),
    Busy(ResourceBusy),
}

pub enum StableResourceTransition<State, Permit, Rejection> {
    Stable { state: State, permit: Permit },
    Rejected(Rejection),
    Busy(ResourceBusy),
}

pub fn stabilize_resource_transition<Token, State, Permit, Rejection, Snapshot, Authorize, Key>(
    operation: &str,
    mut snapshot: Snapshot,
    mut authorize: Authorize,
    primary_key: Key,
) -> KernelResult<StableResourceTransition<State, Permit, Rejection>>
where
    Token: PartialEq,
    Snapshot: FnMut() -> KernelResult<(Token, State)>,
    Authorize: FnMut(&State) -> KernelResult<ResourceTransitionAuthorization<Permit, Rejection>>,
    Key: Fn(&State) -> ResourceKey,
{
    let operation_id = new_operation_id();
    let started = Instant::now();
    let mut last_key = ResourceKey::maintenance();
    for attempt in 1..=STABILIZATION_ATTEMPTS {
        let (before_token, before_state) = snapshot()?;
        last_key = primary_key(&before_state);
        let permit = match authorize(&before_state)? {
            ResourceTransitionAuthorization::Permitted(permit) => permit,
            ResourceTransitionAuthorization::Rejected(rejection) => {
                return Ok(StableResourceTransition::Rejected(rejection));
            }
            ResourceTransitionAuthorization::Busy(busy) => {
                return Ok(StableResourceTransition::Busy(busy));
            }
        };
        let (after_token, after_state) = snapshot()?;
        if before_token == after_token {
            return Ok(StableResourceTransition::Stable {
                state: after_state,
                permit,
            });
        }
        drop(permit);
        if attempt < STABILIZATION_ATTEMPTS {
            thread::sleep(bounded_retry_delay(
                &operation_id,
                std::process::id(),
                attempt,
                25,
                75,
            ));
        }
    }
    Ok(StableResourceTransition::Busy(ResourceBusy {
        code: ResourceCoordinationCode::ResourceStateUnstable,
        operation_id,
        key: last_key,
        attempts: STABILIZATION_ATTEMPTS,
        waited_millis: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        holders: Vec::new(),
        retry_after_millis: 100,
        description: format!(
            "resource state changed repeatedly while authorizing `{operation}`; retry after concurrent updates finish"
        ),
    }))
}

pub fn bounded_retry_delay(
    operation_id: &str,
    process_id: u32,
    attempt: u32,
    min_millis: u64,
    max_millis: u64,
) -> std::time::Duration {
    let min_millis = min_millis.min(max_millis);
    let width = max_millis.saturating_sub(min_millis).saturating_add(1);
    let mut hasher = Sha256::new();
    hasher.update(operation_id.as_bytes());
    hasher.update(process_id.to_le_bytes());
    hasher.update(attempt.to_le_bytes());
    let digest = hasher.finalize();
    let sample = u64::from_le_bytes(digest[..8].try_into().expect("sha256 prefix"));
    std::time::Duration::from_millis(min_millis + sample % width)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn changing_state_releases_each_permit_and_returns_typed_unstable_busy() {
        let reads = Arc::new(Mutex::new(0_u32));
        let releases = Arc::new(Mutex::new(0_u32));
        let result = stabilize_resource_transition(
            "test-transition",
            {
                let reads = Arc::clone(&reads);
                move || {
                    let mut reads = reads.lock().unwrap();
                    *reads += 1;
                    Ok((*reads, *reads))
                }
            },
            {
                let releases = Arc::clone(&releases);
                move |_| {
                    Ok(ResourceTransitionAuthorization::<_, ()>::Permitted(
                        ReleaseCounter(Arc::clone(&releases)),
                    ))
                }
            },
            |_| ResourceKey::maintenance(),
        )
        .unwrap();

        let StableResourceTransition::Busy(busy) = result else {
            panic!("expected unstable busy result");
        };
        assert_eq!(busy.code, ResourceCoordinationCode::ResourceStateUnstable);
        assert_eq!(busy.attempts, 3);
        assert_eq!(*releases.lock().unwrap(), 3);
    }

    #[test]
    fn retry_delay_is_stable_bounded_and_seeded() {
        let first = bounded_retry_delay("operation-a", 11, 1, 25, 75);
        assert_eq!(first, bounded_retry_delay("operation-a", 11, 1, 25, 75));
        assert!((25..=75).contains(&(first.as_millis() as u64)));
        let samples = (1..=20)
            .map(|attempt| bounded_retry_delay("operation-a", 11, attempt, 25, 75))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(samples.len() > 1);
    }

    struct ReleaseCounter(Arc<Mutex<u32>>);

    impl Drop for ReleaseCounter {
        fn drop(&mut self) {
            *self.0.lock().unwrap() += 1;
        }
    }
}
