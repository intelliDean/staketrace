//! Slot, epoch, and timestamp arithmetic utilities.

use crate::constants::{SECONDS_PER_SLOT, SLOTS_PER_EPOCH};

/// Converts a Unix timestamp in seconds to an estimated Consensus Layer slot.
#[inline]
pub fn timestamp_to_slot(timestamp: u64, genesis_time: u64) -> u64 {
    if timestamp < genesis_time {
        0
    } else {
        (timestamp - genesis_time) / SECONDS_PER_SLOT
    }
}

/// Computes the epoch containing a given slot.
#[inline]
pub fn slot_to_epoch(slot: u64) -> u64 {
    slot / SLOTS_PER_EPOCH
}

/// Returns `true` if the block epoch is strictly less than the finalized epoch.
#[inline]
pub fn is_epoch_finalized(block_epoch: u64, finalized_epoch: u64) -> bool {
    block_epoch < finalized_epoch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_to_slot_conversions() {
        let genesis = 1606824023;
        assert_eq!(timestamp_to_slot(genesis, genesis), 0);
        assert_eq!(timestamp_to_slot(genesis + 12, genesis), 1);
        assert_eq!(timestamp_to_slot(genesis + 1200, genesis), 100);
        assert_eq!(timestamp_to_slot(genesis - 10, genesis), 0);
    }

    #[test]
    fn test_slot_to_epoch_and_finality() {
        assert_eq!(slot_to_epoch(0), 0);
        assert_eq!(slot_to_epoch(31), 0);
        assert_eq!(slot_to_epoch(32), 1);
        assert_eq!(slot_to_epoch(100), 3);

        assert!(is_epoch_finalized(3, 10));
        assert!(!is_epoch_finalized(10, 10));
        assert!(!is_epoch_finalized(11, 10));
    }
}
