use super::replacer::Replacer;
use crate::typedef::FrameId;
use std::collections::{HashMap, VecDeque};

/// Represents a node in the LRUKReplacer, maintaining access history and evictability status.
#[derive(Debug)]
struct LrukNode {
    frame_id: FrameId,
    is_evictable: bool,
    history: VecDeque<u64>, // Stores the last K access timestamps
    k: usize,
}

impl LrukNode {
    /// Creates an LRUkNode, which is not evictable by default.
    fn new(frame_id: FrameId, k: usize) -> Self {
        Self {
            frame_id,
            is_evictable: false,
            history: VecDeque::with_capacity(k),
            k,
        }
    }

    /// Checks if the node has an infinite backward K-distance.
    fn has_inf_backward_k_dist(&self) -> bool {
        if self.history.len() < self.k {
            return true;
        }
        false
    }

    /// Gets the earliest recorded timestamp.
    fn get_earliest_timestamp(&self) -> u64 {
        *self.history.front().unwrap()
    }

    /// Calculates the backward K-distance of this node.
    fn get_backwards_k_distance(&self, current_timestamp: u64) -> u64 {

        if self.has_inf_backward_k_dist() {
            return u64::MAX;
        }
        
        current_timestamp - self.get_earliest_timestamp()
    }

    /// Inserts a new access timestamp, maintaining the last K timestamps.
    fn insert_history_timestamp(&mut self, current_timestamp: u64) {
        assert!(self.history.is_empty() || current_timestamp > *self.history.back().unwrap());
        self.history.push_back(current_timestamp);
        if self.history.len() > self.k {
            self.history.pop_front();
        }
    }
}

/// Implements the LRU-K replacement policy.
#[derive(Debug)]
pub(crate) struct LrukReplacer {
    node_store: HashMap<FrameId, LrukNode>,
    evictable_size: usize, // Number of evictable nodes
    current_timestamp: u64,
    k: usize, // Number of accesses to track
}

impl LrukReplacer {
    /// Creates a new LRU-K replacer instance.
    pub(crate) fn new(k: usize) -> Self {
        LrukReplacer {
            node_store: HashMap::new(),
            evictable_size: 0,
            current_timestamp: 0,
            k,
        }
    }

    /// Increments and returns the current timestamp.
    fn advance_timestamp(&mut self) -> u64 {
        let old_timestamp = self.current_timestamp;
        self.current_timestamp += 1;
        old_timestamp
    }
}

impl Replacer for LrukReplacer {
    /// Records access to a frame and updates its history.
    fn record_access(&mut self, frame_id: FrameId) {
        
        if !self.node_store.contains_key(&frame_id) {
            let new_node = LrukNode::new(frame_id, self.k);
            self.node_store.insert(frame_id, new_node);
        }
        
        let accessed_frame = self.node_store.get_mut(&frame_id).unwrap();
        accessed_frame.insert_history_timestamp(self.current_timestamp);
        self.advance_timestamp();
    }

    /// Pins a frame, making it non-evictable.
    fn pin(&mut self, frame_id: FrameId) {

        if !self.node_store.contains_key(&frame_id) {
            return;
        }
        
        let accessed_frame = self.node_store.get_mut(&frame_id).unwrap();
        
        if accessed_frame.is_evictable == true {
            accessed_frame.is_evictable = false;

            self.evictable_size -= 1;  
        }
        

    }

    /// Unpins a frame, making it evictable.
    fn unpin(&mut self, frame_id: FrameId) {
        
        if !self.node_store.contains_key(&frame_id) {
            return;
        }
        
        let accessed_frame = self.node_store.get_mut(&frame_id).unwrap();
        
        if accessed_frame.is_evictable == false {
            accessed_frame.is_evictable = true;
            self.evictable_size += 1; 
        }


    }

    /// Evicts the frame with the largest backward k-distance.
    fn evict(&mut self) -> Option<FrameId> {
        let mut max_frame_id = 0;
        let mut max_k_dist = 0;
        let mut earliest = u64::MAX;
        let mut modded = false;
        
        for (frame_id, node) in self.node_store.iter() {
            if node.is_evictable == true && (node.get_backwards_k_distance(self.current_timestamp) > max_k_dist) {
                max_k_dist = node.get_backwards_k_distance(self.current_timestamp);
                max_frame_id = *frame_id;
                earliest = node.get_earliest_timestamp();
                modded = true;
            }

            else if node.is_evictable == true && (node.get_backwards_k_distance(self.current_timestamp) == max_k_dist) {
                if node.get_earliest_timestamp() < earliest {
                    max_k_dist = node.get_backwards_k_distance(self.current_timestamp);
                    max_frame_id = *frame_id;
                    earliest = node.get_earliest_timestamp();
                    modded = true;
                }
            }
            
        }
        
        if modded {
            self.node_store.remove(&max_frame_id);
            self.evictable_size -= 1;
            return Some(max_frame_id)
        }
        
        else {
            return None
        }

    }

    /// Removes a frame from the replacer if it is evictable.
    fn remove(&mut self, frame_id: FrameId) {

        if !self.node_store.contains_key(&frame_id) {
            return;
        }
        
        let accessed_frame = self.node_store.get_mut(&frame_id).unwrap();
        if accessed_frame.is_evictable {
            self.node_store.remove(&frame_id);
            self.evictable_size -= 1;
        }
    }

    /// Returns the number of evictable frames.
    fn evictable_count(&self) -> usize {
        self.evictable_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lruk_replacer_one() {
        let mut lru_replacer = LrukReplacer::new(2);

        lru_replacer.record_access(1);
        lru_replacer.record_access(2);
        lru_replacer.record_access(3);
        lru_replacer.record_access(4);
        lru_replacer.record_access(5);
        lru_replacer.record_access(6);
        lru_replacer.unpin(1);
        lru_replacer.unpin(2);
        lru_replacer.unpin(3);
        lru_replacer.unpin(4);
        lru_replacer.unpin(5);
        lru_replacer.pin(6);

        assert_eq!(5, lru_replacer.evictable_count());

        lru_replacer.record_access(1);
        assert_eq!(Some(2), lru_replacer.evict());
        assert_eq!(Some(3), lru_replacer.evict());
        assert_eq!(Some(4), lru_replacer.evict());
        assert_eq!(2, lru_replacer.evictable_count());

        lru_replacer.record_access(3);
        lru_replacer.record_access(4);
        lru_replacer.record_access(5);
        lru_replacer.record_access(4);
        lru_replacer.unpin(3);
        lru_replacer.unpin(4);
        assert_eq!(4, lru_replacer.evictable_count());

        assert_eq!(Some(3), lru_replacer.evict());
        assert_eq!(3, lru_replacer.evictable_count());

        lru_replacer.unpin(6);
        assert_eq!(4, lru_replacer.evictable_count());
        assert_eq!(Some(6), lru_replacer.evict());
        assert_eq!(3, lru_replacer.evictable_count());

        lru_replacer.pin(1);
        assert_eq!(2, lru_replacer.evictable_count());
        assert_eq!(Some(5), lru_replacer.evict());
        assert_eq!(1, lru_replacer.evictable_count());

        lru_replacer.record_access(1);
        lru_replacer.record_access(1);
        lru_replacer.unpin(1);
        assert_eq!(2, lru_replacer.evictable_count());

        assert_eq!(Some(4), lru_replacer.evict());
        assert_eq!(1, lru_replacer.evictable_count());
        assert_eq!(Some(1), lru_replacer.evict());
        assert_eq!(0, lru_replacer.evictable_count());

        lru_replacer.record_access(1);
        lru_replacer.pin(1);
        assert_eq!(0, lru_replacer.evictable_count());

        assert_eq!(None, lru_replacer.evict());

        lru_replacer.unpin(1);
        assert_eq!(1, lru_replacer.evictable_count());
        assert_eq!(Some(1), lru_replacer.evict());
        assert_eq!(0, lru_replacer.evictable_count());

        assert_eq!(None, lru_replacer.evict());
        assert_eq!(0, lru_replacer.evictable_count());

        lru_replacer.pin(6);
        lru_replacer.unpin(6);
    }

    #[test]
    fn test_lruk_replacer_two() {
        let mut lru_replacer = LrukReplacer::new(2);

        // Add six frames to the replacer. Frame 6 is non-evictable.
        lru_replacer.record_access(1);
        lru_replacer.record_access(2);
        lru_replacer.record_access(3);
        lru_replacer.record_access(4);
        lru_replacer.record_access(5);
        lru_replacer.record_access(6);
        lru_replacer.unpin(1);
        lru_replacer.unpin(2);
        lru_replacer.unpin(3);
        lru_replacer.unpin(4);
        lru_replacer.unpin(5);
        lru_replacer.pin(6);

        // The size of the replacer is the number of evictable frames
        assert_eq!(5, lru_replacer.evictable_count());

        // Record an access for frame 1
        lru_replacer.record_access(1);

        // Evict three pages
        assert_eq!(Some(2), lru_replacer.evict());
        assert_eq!(Some(3), lru_replacer.evict());
        assert_eq!(Some(4), lru_replacer.evict());
        assert_eq!(2, lru_replacer.evictable_count());

        // Insert new frames [3, 4] and update history
        lru_replacer.record_access(3);
        lru_replacer.record_access(4);
        lru_replacer.record_access(5);
        lru_replacer.record_access(4);
        lru_replacer.unpin(3);
        lru_replacer.unpin(4);
        assert_eq!(4, lru_replacer.evictable_count());

        // Expect frame 3 to be evicted next
        assert_eq!(Some(3), lru_replacer.evict());
        assert_eq!(3, lru_replacer.evictable_count());

        // Set frame 6 to be evictable and evict it
        lru_replacer.unpin(6);
        assert_eq!(4, lru_replacer.evictable_count());
        assert_eq!(Some(6), lru_replacer.evict());
        assert_eq!(3, lru_replacer.evictable_count());

        // Mark frame 1 as non-evictable
        lru_replacer.pin(1);
        assert_eq!(2, lru_replacer.evictable_count());

        // Expect frame 5 to be evicted next
        assert_eq!(Some(5), lru_replacer.evict());
        assert_eq!(1, lru_replacer.evictable_count());

        // Update history for frame 1 and make it evictable
        lru_replacer.record_access(1);
        lru_replacer.record_access(1);
        lru_replacer.unpin(1);
        assert_eq!(2, lru_replacer.evictable_count());

        // Evict the last two frames
        assert_eq!(Some(4), lru_replacer.evict());
        assert_eq!(1, lru_replacer.evictable_count());
        assert_eq!(Some(1), lru_replacer.evict());
        assert_eq!(0, lru_replacer.evictable_count());

        // Insert frame 1 again and mark it as non-evictable
        lru_replacer.record_access(1);
        lru_replacer.pin(1);
        assert_eq!(0, lru_replacer.evictable_count());

        // A failed eviction should not change the size of the replacer
        assert_eq!(None, lru_replacer.evict());

        // Mark frame 1 as evictable again and evict it
        lru_replacer.unpin(1);
        assert_eq!(1, lru_replacer.evictable_count());
        assert_eq!(Some(1), lru_replacer.evict());
        assert_eq!(0, lru_replacer.evictable_count());

        // Ensure that eviction on an empty replacer does nothing strange
        assert_eq!(None, lru_replacer.evict());
        assert_eq!(0, lru_replacer.evictable_count());

        // Ensure setting a non-existent frame as evictable does not cause issues
        lru_replacer.unpin(6);
    }

    #[test]
    fn test_lruk_replacer_evict() {
        {
            // Empty and try removing
            let mut lru_replacer = LrukReplacer::new(2);
            assert_eq!(None, lru_replacer.evict());
        }

        {
            // Can only evict element if evictable=true
            let mut lru_replacer = LrukReplacer::new(2);
            lru_replacer.record_access(2);
            lru_replacer.pin(2);
            assert_eq!(None, lru_replacer.evict());
            lru_replacer.unpin(2);
            assert_eq!(Some(2), lru_replacer.evict());
        }

        {
            // Elements with less than k history should have max backward k-dist and get evicted first
            let mut lru_replacer = LrukReplacer::new(3);
            lru_replacer.record_access(1);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.record_access(1);
            lru_replacer.unpin(2);
            lru_replacer.unpin(1);

            assert_eq!(Some(2), lru_replacer.evict());
            assert_eq!(Some(1), lru_replacer.evict());
        }

        {
            // Select element with largest backward k-dist to evict
            let mut lru_replacer = LrukReplacer::new(3);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.record_access(3);
            lru_replacer.record_access(3);
            lru_replacer.record_access(3);
            lru_replacer.record_access(2);
            lru_replacer.record_access(2);
            lru_replacer.record_access(1);
            lru_replacer.record_access(1);
            lru_replacer.record_access(3);
            lru_replacer.record_access(2);
            lru_replacer.record_access(1);
            lru_replacer.unpin(2);
            lru_replacer.unpin(1);
            lru_replacer.unpin(3);

            assert_eq!(Some(3), lru_replacer.evict());
            assert_eq!(Some(2), lru_replacer.evict());
            assert_eq!(Some(1), lru_replacer.evict());
        }

        {
            let mut lru_replacer = LrukReplacer::new(3);
            lru_replacer.record_access(2);
            lru_replacer.record_access(2);
            lru_replacer.record_access(2);
            lru_replacer.record_access(1);
            lru_replacer.record_access(1);
            lru_replacer.unpin(2);
            lru_replacer.unpin(1);

            assert_eq!(Some(1), lru_replacer.evict());

            lru_replacer.record_access(1);
            lru_replacer.unpin(1);

            assert_eq!(Some(1), lru_replacer.evict());
        }

        {
            let mut lru_replacer = LrukReplacer::new(3);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.record_access(3);
            lru_replacer.record_access(4);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.record_access(3);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.unpin(1);
            lru_replacer.unpin(2);
            lru_replacer.unpin(3);
            lru_replacer.unpin(4);

            assert_eq!(Some(3), lru_replacer.evict());
            lru_replacer.record_access(4);
            lru_replacer.record_access(4);

            assert_eq!(Some(1), lru_replacer.evict());
            assert_eq!(Some(2), lru_replacer.evict());
            assert_eq!(Some(4), lru_replacer.evict());
        }

        {
            let mut lru_replacer = LrukReplacer::new(2);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.record_access(3);
            lru_replacer.record_access(4);
            lru_replacer.record_access(1);
            lru_replacer.record_access(2);
            lru_replacer.record_access(3);
            lru_replacer.record_access(4);

            lru_replacer.unpin(2);
            lru_replacer.unpin(1);

            assert_eq!(Some(1), lru_replacer.evict());

            lru_replacer.record_access(5);
            lru_replacer.unpin(5);
            assert_eq!(Some(5), lru_replacer.evict());
        }

        {
            let mut lru_replacer = LrukReplacer::new(3);
            for j in 0..4 {
                for i in (j * 250)..1000 {
                    lru_replacer.record_access(i);
                    lru_replacer.unpin(i);
                }
            }
            assert_eq!(1000, lru_replacer.evictable_count());

            for i in 250..500 {
                lru_replacer.pin(i);
            }
            assert_eq!(750, lru_replacer.evictable_count());

            for i in 0..100 {
                lru_replacer.remove(i);
            }
            assert_eq!(650, lru_replacer.evictable_count());

            for i in 100..600 {
                if i < 250 || i >= 500 {
                    assert_eq!(Some(i), lru_replacer.evict());
                }
            }
            assert_eq!(400, lru_replacer.evictable_count());

            for i in 250..500 {
                lru_replacer.unpin(i);
            }
            assert_eq!(650, lru_replacer.evictable_count());

            for i in 600..750 {
                lru_replacer.record_access(i);
                lru_replacer.record_access(i);
            }
            assert_eq!(650, lru_replacer.evictable_count());

            for i in 250..500 {
                assert_eq!(Some(i), lru_replacer.evict());
            }
            assert_eq!(400, lru_replacer.evictable_count());

            for i in 750..1000 {
                assert_eq!(Some(i), lru_replacer.evict());
            }
            assert_eq!(150, lru_replacer.evictable_count());

            for i in 600..750 {
                assert_eq!(Some(i), lru_replacer.evict());
            }
            assert_eq!(0, lru_replacer.evictable_count());
        }
    }

}
