use std::sync::atomic::{AtomicU64, Ordering};

/// Safety epoch + reconciliation claim for emergency unlock (HF-01).
///
/// The `WH_KEYBOARD_LL` callback must never block: it cannot wait on the
/// bounded UI event queue, touch the config file, or emit to the UI. Instead
/// the hook records the emergency with a single atomic increment and wakes a
/// dedicated worker thread over an unbounded channel. The worker (with the
/// event pump as a fallback) then converges config/UI/tray exactly once per
/// epoch via [`SafetyState::claim_reconcile`].
///
/// The same epoch also serializes mutating commands: a profile/key-disable
/// command that was in flight while an emergency landed observes a newer
/// epoch and is discarded instead of re-disabling keys after the unlock.
#[derive(Debug, Default)]
pub struct SafetyState {
    /// Bumped by the hook thread on every emergency press. Starts at 0;
    /// the first trigger yields epoch 1.
    epoch: AtomicU64,
    /// Highest epoch fully converged (config + controller + persist + UI).
    /// Written exactly once per epoch through [`SafetyState::claim_reconcile`].
    reconciled: AtomicU64,
}

impl SafetyState {
    pub fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            reconciled: AtomicU64::new(0),
        }
    }

    /// Record an emergency press. Hook-thread only: a single atomic
    /// increment, never blocks, never touches config/files/UI.
    /// Returns the new epoch.
    pub fn trigger(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Latest epoch the hook has recorded.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Highest epoch already converged.
    pub fn reconciled_epoch(&self) -> u64 {
        self.reconciled.load(Ordering::SeqCst)
    }

    /// Try to win the right to converge `epoch`. Returns `true` exactly once
    /// per epoch across all threads (safety worker, event pump, stale-command
    /// path); older or already-handled epochs return `false`.
    pub fn claim_reconcile(&self, epoch: u64) -> bool {
        let mut current = self.reconciled.load(Ordering::SeqCst);
        loop {
            if epoch <= current {
                return false;
            }
            match self.reconciled.compare_exchange_weak(
                current,
                epoch,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn trigger_increments_epoch_from_one() {
        let safety = SafetyState::new();
        assert_eq!(safety.epoch(), 0);
        assert_eq!(safety.trigger(), 1);
        assert_eq!(safety.trigger(), 2);
        assert_eq!(safety.epoch(), 2);
    }

    #[test]
    fn claim_is_exactly_once_per_epoch() {
        let safety = SafetyState::new();
        let epoch = safety.trigger();
        assert!(safety.claim_reconcile(epoch));
        // Second claim of the same epoch loses, from any thread.
        assert!(!safety.claim_reconcile(epoch));
        assert_eq!(safety.reconciled_epoch(), epoch);
    }

    #[test]
    fn stale_epoch_cannot_claim() {
        let safety = SafetyState::new();
        safety.trigger(); // epoch 1
        let epoch2 = safety.trigger(); // epoch 2
        assert!(safety.claim_reconcile(epoch2));
        // Epoch 1 arrives late (e.g. a delayed pump event): must not
        // re-run convergence for already-handled state.
        assert!(!safety.claim_reconcile(1));
        assert_eq!(safety.reconciled_epoch(), epoch2);
    }

    #[test]
    fn rapid_triggers_converge_to_latest() {
        let safety = SafetyState::new();
        for _ in 0..100 {
            safety.trigger();
        }
        let latest = safety.epoch();
        assert_eq!(latest, 100);
        assert!(safety.claim_reconcile(latest));
        assert!(!safety.claim_reconcile(latest));
    }

    #[test]
    fn concurrent_triggers_have_unique_epochs() {
        let safety = Arc::new(SafetyState::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let safety = safety.clone();
            handles.push(std::thread::spawn(move || {
                let mut seen = Vec::new();
                for _ in 0..25 {
                    seen.push(safety.trigger());
                }
                seen
            }));
        }
        let mut all: Vec<u64> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 200);
        assert_eq!(safety.epoch(), 200);
    }

    #[test]
    fn concurrent_claims_elect_exactly_one_winner() {
        let safety = Arc::new(SafetyState::new());
        let epoch = safety.trigger();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let safety = safety.clone();
            handles.push(std::thread::spawn(move || safety.claim_reconcile(epoch)));
        }
        let wins: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap() as usize)
            .sum();
        assert_eq!(wins, 1);
    }
}
