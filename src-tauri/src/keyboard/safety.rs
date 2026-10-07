use parking_lot::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicU64, Ordering};

/// Safety epoch + reconciliation protocol for emergency unlock (HF-01, P2).
///
/// The `WH_KEYBOARD_LL` callback must never block: it cannot wait on the
/// bounded UI event queue, touch the config file, or emit to the UI. Instead
/// the hook records the emergency with a single atomic increment and wakes a
/// dedicated worker thread over an unbounded channel. The worker (with the
/// event pump as a fallback) then converges config/controller/persist/UI.
///
/// Protocol (all convergence sites: safety worker, event-pump fallback,
/// stale-command path):
///
/// 1. Decide: `pending_epoch()` returns the latest epoch that still needs
///    convergence, or `None` when fully converged.
/// 2. Serialize: hold `convergence_lock()` (blocking) or
///    `try_convergence_lock()` (never blocks; for the UI thread) while
///    working, so convergence work for different epochs never overlaps.
/// 3. Elect: `claim_reconcile(epoch)` records which thread took
///    responsibility (observability; exactly-once election per epoch).
/// 4. Work: run the idempotent `emergency_unlock()`.
/// 5. Complete: `complete_reconcile(epoch)` advances the truthful completion
///    marker. It must run only after the work finished.
///
/// The same epoch also serializes mutating commands: a profile/key-disable
/// command that was in flight while an emergency landed observes a newer
/// epoch and is discarded instead of re-disabling keys after the unlock.
///
/// Invariants:
/// - `reconciled_epoch()` (the completion marker) never advances before the
///   convergence work for that epoch actually finished. Claiming is election
///   only; it does not imply completion.
/// - `completed` is monotonic: a stale (older) completion never moves the
///   marker backwards.
/// - Convergence work is mutually exclusive across all paths.
#[derive(Debug, Default)]
pub struct SafetyState {
    /// Bumped by the hook thread on every emergency press. Starts at 0;
    /// the first trigger yields epoch 1.
    epoch: AtomicU64,
    /// Election marker: highest epoch some thread took responsibility for.
    /// Winning the election does NOT mean the work is done; see `completed`.
    claimed: AtomicU64,
    /// Completion marker: highest epoch whose config/controller/persist
    /// convergence actually finished. Written only by `complete_reconcile`,
    /// after the work, while holding the convergence lock.
    completed: AtomicU64,
    /// Serializes convergence work across the safety worker, the event-pump
    /// fallback, and the stale-command path. The hook callback never touches
    /// this lock.
    convergence: Mutex<()>,
}

impl SafetyState {
    pub fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            claimed: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            convergence: Mutex::new(()),
        }
    }

    /// Record an emergency press. Hook-thread only: a single atomic
    /// increment, never blocks, never touches config/files/UI/locks.
    /// Returns the new epoch.
    pub fn trigger(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Latest epoch the hook has recorded.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Highest epoch whose convergence work fully completed. This is the
    /// only truthful "done" signal.
    pub fn reconciled_epoch(&self) -> u64 {
        self.completed.load(Ordering::SeqCst)
    }

    /// Highest epoch some thread took responsibility for (election only).
    pub fn claimed_epoch(&self) -> u64 {
        self.claimed.load(Ordering::SeqCst)
    }

    /// Latest epoch still needing convergence, or `None` when the state is
    /// fully converged. This is what the safety worker loops on: unlike the
    /// old claim-then-skip pattern, an epoch elected-but-never-worked is
    /// still reported as pending, so it can never be silently lost.
    pub fn pending_epoch(&self) -> Option<u64> {
        let target = self.epoch.load(Ordering::SeqCst);
        if self.completed.load(Ordering::SeqCst) >= target {
            None
        } else {
            Some(target)
        }
    }

    /// Try to win the election to converge `epoch`. Returns `true` exactly
    /// once per epoch across all threads. Election only: the winner must
    /// still run the work under `convergence_lock()` and then call
    /// `complete_reconcile`. (Kept for the pump/stale paths and tests.)
    pub fn claim_reconcile(&self, epoch: u64) -> bool {
        let mut current = self.claimed.load(Ordering::SeqCst);
        loop {
            if epoch <= current {
                return false;
            }
            match self.claimed.compare_exchange_weak(
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

    /// Mark the convergence work for `epoch` as finished. Call only after
    /// the work actually ran, while holding the convergence lock. Monotonic:
    /// a stale (older) completion never moves the marker backwards.
    pub fn complete_reconcile(&self, epoch: u64) {
        self.completed.fetch_max(epoch, Ordering::SeqCst);
    }

    /// Acquire the convergence lock, blocking. For background threads only
    /// (safety worker, command pool); never call from the hook callback or
    /// the UI thread.
    pub fn convergence_lock(&self) -> MutexGuard<'_, ()> {
        self.convergence.lock()
    }

    /// Try to acquire the convergence lock without blocking. For the UI
    /// thread (event-pump fallback): if the worker is already converging,
    /// skip and let the worker finish — it loops on `pending_epoch()`.
    pub fn try_convergence_lock(&self) -> Option<MutexGuard<'_, ()>> {
        self.convergence.try_lock()
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
        assert_eq!(safety.claimed_epoch(), epoch);
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
        assert_eq!(safety.claimed_epoch(), epoch2);
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

    // ---- P2: claim/completion separation ----

    /// P2 core defect: winning the election must not advance the completion
    /// marker before the config/controller work actually ran. (On the old
    /// code this failed: `claim_reconcile` set the reconciled marker
    /// immediately, so `reconciled_epoch()` lied about completion.)
    #[test]
    fn reconciled_epoch_does_not_advance_before_work_completes() {
        let safety = SafetyState::new();
        let epoch = safety.trigger();
        assert!(safety.claim_reconcile(epoch));
        assert_eq!(
            safety.reconciled_epoch(),
            0,
            "claim is election only; completion marker must wait for complete_reconcile"
        );
        // The work runs here (under the convergence lock in real paths)...
        safety.complete_reconcile(epoch);
        assert_eq!(safety.reconciled_epoch(), epoch);
    }

    /// An epoch that was elected but never worked must still be reported as
    /// pending, so another path converges it instead of silently dropping it.
    /// (On the old code the worker skipped on claim-failure, losing the epoch.)
    #[test]
    fn pending_epoch_survives_claim_without_work() {
        let safety = SafetyState::new();
        safety.trigger(); // epoch 1
        let epoch2 = safety.trigger(); // epoch 2
        // Some path wins the election but defers the work (e.g. its
        // try_lock failed and it bailed out).
        assert!(safety.claim_reconcile(epoch2));
        assert_eq!(safety.pending_epoch(), Some(epoch2));
        assert_eq!(safety.reconciled_epoch(), 0);
        // A worker looping on pending_epoch() converges it:
        {
            let _guard = safety.convergence_lock();
            safety.complete_reconcile(epoch2); // work simulated here
        }
        assert_eq!(safety.pending_epoch(), None);
        assert_eq!(safety.reconciled_epoch(), epoch2);
    }

    /// Completion is monotonic: a stale (older) completion never moves the
    /// marker backwards, even if an old epoch's slow work finishes late.
    #[test]
    fn complete_reconcile_is_monotonic() {
        let safety = SafetyState::new();
        safety.trigger(); // 1
        safety.trigger(); // 2
        safety.complete_reconcile(2);
        assert_eq!(safety.reconciled_epoch(), 2);
        safety.complete_reconcile(1); // late straggler
        assert_eq!(safety.reconciled_epoch(), 2);
        assert_eq!(safety.pending_epoch(), None);
    }

    /// Convergence work is mutually exclusive: while one path holds the
    /// lock (slow persist, stalled disk), another path must not enter.
    /// Deterministic: channel-coordinated, no sleeps.
    #[test]
    fn convergence_lock_serializes_worker_and_pump() {
        let safety = Arc::new(SafetyState::new());
        let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        // "Worker": takes the lock and holds it until told to release.
        let worker_safety = safety.clone();
        let worker = std::thread::spawn(move || {
            let _guard = worker_safety.convergence_lock();
            held_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            // _guard dropped here, releasing the lock
        });
        held_rx.recv().unwrap(); // worker holds the lock now
        // "Pump" (UI thread) must not enter while the worker holds it.
        assert!(safety.try_convergence_lock().is_none());
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        // After release, the pump path can proceed.
        assert!(safety.try_convergence_lock().is_some());
    }

    /// The worker-loop protocol from lib.rs, without Tauri: rapid triggers
    /// coalesce into a single convergence of the latest epoch.
    #[test]
    fn worker_protocol_converges_latest_epoch_exactly_once() {
        let safety = SafetyState::new();
        for _ in 0..5 {
            safety.trigger();
        }
        let mut work_count = 0u32;
        loop {
            if safety.pending_epoch().is_none() {
                break;
            }
            {
                let _guard = safety.convergence_lock();
                // Re-check under the lock: another path may have converged.
                if safety.reconciled_epoch() >= safety.epoch() {
                    break;
                }
                let target = safety.epoch();
                let _elected = safety.claim_reconcile(target);
                work_count += 1; // state.emergency_unlock() runs here
                safety.complete_reconcile(target);
            }
            // Loop again: a newer epoch may have landed during the work.
        }
        assert_eq!(work_count, 1, "five rapid triggers coalesce to one convergence");
        assert_eq!(safety.reconciled_epoch(), 5);
        assert_eq!(safety.pending_epoch(), None);
        assert_eq!(safety.claimed_epoch(), 5);
    }

    /// A newer trigger landing mid-work is picked up by the next loop
    /// iteration instead of being lost.
    #[test]
    fn newer_epoch_during_work_is_not_lost() {
        let safety = SafetyState::new();
        safety.trigger(); // epoch 1
        // Iteration 1: worker starts converging epoch 1...
        let target = safety.pending_epoch().expect("epoch 1 pending");
        assert_eq!(target, 1);
        {
            let _guard = safety.convergence_lock();
            // ...a second press lands mid-work.
            let epoch2 = safety.trigger();
            assert_eq!(epoch2, 2);
            safety.claim_reconcile(target);
            // work for epoch 1 finishes here
            safety.complete_reconcile(target);
        }
        // Next iteration sees epoch 2 still pending.
        assert_eq!(safety.pending_epoch(), Some(2));
        {
            let _guard = safety.convergence_lock();
            let target = safety.epoch();
            safety.claim_reconcile(target);
            safety.complete_reconcile(target);
        }
        assert_eq!(safety.pending_epoch(), None);
        assert_eq!(safety.reconciled_epoch(), 2);
    }
}
