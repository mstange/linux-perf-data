use std::collections::VecDeque;

/// Accumulates unordered key-value pairs and emits them in order, sorted by the key.
///
/// The caller can indicate "rounds" with the property that round N cannot
/// overlap with round N + 2. In other words, the lowest key in round N + 2
/// must be greater than or equal to the highest key in round N.
///
/// Every time a round is finished, some values become available for ordered
/// iteration, specifically those values whose order cannot be affected by
/// upcoming values due to the overlap guarantee.
//
// Implementation notes:
//
//   i: incoming values (unordered)
//   o: outgoing values (ordered)
//
// Round 1:          |<============>|  // insert_unordered is called with unordered keys in this range
//                   ^              ^--- cur_max
//                   `------------------ prev_max
//  finish_round()   iiiiiiiiiiiiiiii  // nothing is available in outgoing yet, everything is still incoming
//                                  ^--- cur_max
//                                  ^--- prev_max
// Round 2:                 |<======================>|  // more insert_unordered calls
//                                  ^                ^--- cur_max
//                                  `-------------------- prev_max
//  finish_round()   ooooooooooooooooiiiiiiiiiiiiiiiii  // everything <= prev_max is moved to outgoing
//                                                   ^--- cur_max
//                                                   ^--- prev_max
// Round 3:                            |<================>|  // more insert_unordered calls, no overlap with round 1
//                                                   ^    ^--- cur_max
//                                                   `-------- prev_max
//  finish_round()                   oooooooooooooooooiiiii  // everything <= prev_max is moved to outgoing
#[derive(Debug, Clone)]
pub struct Sorter<K: Ord + Default, V> {
    /// This list is ordered and all values are <= prev_max.
    outgoing: VecDeque<V>,
    /// Unsorted values.
    incoming: VecDeque<(K, V)>,
    /// The maximum key of incoming in previous round.
    prev_max: K,
    /// The maximum key of incoming in the current round.
    cur_max: K,
    /// The number of values in incoming which are <= prev_max.
    incoming_lte_prev_max_count: usize,
}

impl<K: Ord + Clone + Default, V> Default for Sorter<K, V> {
    fn default() -> Self {
        Self {
            outgoing: VecDeque::new(),
            incoming: VecDeque::new(),
            prev_max: Default::default(),
            cur_max: Default::default(),
            incoming_lte_prev_max_count: 0,
        }
    }
}

impl<K: Ord + Clone + Default, V> Sorter<K, V> {
    /// Create a new sorter.
    pub fn new() -> Self {
        Default::default()
    }

    /// Whether there are more ordered values available. If this returns false,
    /// the next round must be read.
    pub fn has_more(&self) -> bool {
        !self.outgoing.is_empty()
    }

    /// Returns values in order.
    ///
    /// The order is only guaranteed if the caller respected the contract for
    /// `insert_unordered`.
    pub fn get_next(&mut self) -> Option<V> {
        self.outgoing.pop_front()
    }

    /// Insert an element. The caller guarantees that `key` is at least as large
    /// as the largest key seen two `finish_round` calls ago. In other words, round
    /// N must not overlap with round N - 2.
    pub fn insert_unordered(&mut self, key: K, value: V) {
        if key <= self.prev_max {
            self.incoming_lte_prev_max_count += 1;
        } else if key > self.cur_max {
            self.cur_max = key.clone();
        }
        self.incoming.push_back((key, value));
    }

    /// Finish the current round. This makes some of the inserted values available
    /// from `get_next`, specifically any values which cannot have their order affected
    /// by values from the next round.
    pub fn finish_round(&mut self) {
        if let Some(n) = self.incoming_lte_prev_max_count.checked_sub(1) {
            let (new_outgoing, _middle, _remaining) = self
                .incoming
                .make_contiguous()
                .select_nth_unstable_by_key(n, |(key, _value)| key.clone());
            new_outgoing.sort_unstable_by_key(|(key, _value)| key.clone());

            // Move everything <= prev_max from incoming into outgoing.
            for _ in 0..self.incoming_lte_prev_max_count {
                let (_key, value) = self.incoming.pop_front().unwrap();
                self.outgoing.push_back(value);
            }
        }

        self.prev_max = self.cur_max.clone();
        self.incoming_lte_prev_max_count = self.incoming.len();
    }

    /// Finish all rounds and declare that no more values will be inserted after this call.
    /// This makes all inserted values available from `get_next()`.
    pub fn finish(&mut self) {
        self.incoming
            .make_contiguous()
            .sort_unstable_by_key(|(key, _value)| key.clone());

        while let Some((_key, value)) = self.incoming.pop_front() {
            self.outgoing.push_back(value);
        }
        self.prev_max = self.cur_max.clone();
    }
}

#[cfg(test)]
mod test {
    use super::Sorter;

    // Example from the perf FINISHED_ROUND docs:
    //
    //    ============ PASS n =================
    //       CPU 0         |   CPU 1
    //                     |
    //    cnt1 timestamps  |   cnt2 timestamps
    //          1          |         2
    //          2          |         3
    //          -          |         4  <--- max recorded
    //
    //    ============ PASS n + 1 ==============
    //       CPU 0         |   CPU 1
    //                     |
    //    cnt1 timestamps  |   cnt2 timestamps
    //          3          |         5
    //          4          |         6
    //          5          |         7 <---- max recorded
    //
    //      Flush every events below timestamp 4
    //
    //    ============ PASS n + 2 ==============
    //       CPU 0         |   CPU 1
    //                     |
    //    cnt1 timestamps  |   cnt2 timestamps
    //          6          |         8
    //          7          |         9
    //          -          |         10
    //
    //      Flush every events below timestamp 7
    //      etc...
    #[test]
    fn it_works() {
        let mut sorter = Sorter::new();
        sorter.insert_unordered(1, "1"); // cpu 0
        sorter.insert_unordered(2, "2"); // cpu 1
        sorter.insert_unordered(3, "3"); // cpu 1
        sorter.insert_unordered(2, "2"); // cpu 0
        sorter.insert_unordered(4, "4"); // cpu 1
        assert_eq!(sorter.get_next(), None);
        sorter.finish_round();
        assert_eq!(sorter.get_next(), None);
        sorter.insert_unordered(3, "3"); // cpu 0
        sorter.insert_unordered(5, "5"); // cpu 1
        sorter.insert_unordered(6, "6"); // cpu 1
        sorter.insert_unordered(7, "7"); // cpu 1
        sorter.insert_unordered(4, "4"); // cpu 0
        sorter.insert_unordered(5, "5"); // cpu 0
        assert_eq!(sorter.get_next(), None);
        sorter.finish_round();
        assert_eq!(sorter.get_next(), Some("1"));
        assert_eq!(sorter.get_next(), Some("2"));
        assert_eq!(sorter.get_next(), Some("2"));
        assert_eq!(sorter.get_next(), Some("3"));
        assert_eq!(sorter.get_next(), Some("3"));
        assert_eq!(sorter.get_next(), Some("4"));
        assert_eq!(sorter.get_next(), Some("4"));
        assert_eq!(sorter.get_next(), None);
        sorter.insert_unordered(6, "6"); // cpu 0
        sorter.insert_unordered(8, "8"); // cpu 1
        sorter.insert_unordered(9, "9"); // cpu 1
        sorter.insert_unordered(7, "7"); // cpu 0
        sorter.insert_unordered(10, "10"); // cpu 1
        assert_eq!(sorter.get_next(), None);
        sorter.finish_round();
        assert_eq!(sorter.get_next(), Some("5"));
        assert_eq!(sorter.get_next(), Some("5"));
        assert_eq!(sorter.get_next(), Some("6"));
        assert_eq!(sorter.get_next(), Some("6"));
        assert_eq!(sorter.get_next(), Some("7"));
        assert_eq!(sorter.get_next(), Some("7"));
        assert_eq!(sorter.get_next(), None);
        sorter.finish();
        assert_eq!(sorter.get_next(), Some("8"));
        assert_eq!(sorter.get_next(), Some("9"));
        assert_eq!(sorter.get_next(), Some("10"));
        assert_eq!(sorter.get_next(), None);
    }
}
