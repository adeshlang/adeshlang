//! ACK range encoding and decoding.

use super::config::MAX_ACK_RANGES;
use super::errors::{AtpError, AtpResult};
use std::collections::BTreeSet;

/// Encode received packet numbers into ACK frame fields.
pub fn encode_ack_ranges(received: &BTreeSet<u64>) -> AtpResult<(u64, Vec<(u64, u64)>)> {
    if received.is_empty() {
        return Err(AtpError::protocol("cannot encode empty ACK set"));
    }

    let largest_acked = *received.iter().next_back().unwrap();
    let mut nums: Vec<u64> = received.iter().copied().collect();
    nums.sort_unstable_by(|a, b| b.cmp(a));

    // Collect contiguous block ranges as (top, bottom) inclusive.
    let mut blocks: Vec<(u64, u64)> = Vec::new();
    let mut i = 0;
    while i < nums.len() {
        let top = nums[i];
        let mut bottom = top;
        let mut j = i + 1;
        while j < nums.len() && nums[j] == bottom - 1 {
            bottom = nums[j];
            j += 1;
        }
        blocks.push((top, bottom));
        i = j;
    }

    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for (idx, (top, bottom)) in blocks.iter().enumerate() {
        if ranges.len() >= MAX_ACK_RANGES {
            break;
        }
        let range_len = top - bottom;
        if idx == 0 {
            ranges.push((0, range_len));
        } else {
            let prev_bottom = blocks[idx - 1].1;
            let gap = prev_bottom.saturating_sub(*top + 1);
            ranges.push((gap, range_len));
        }
    }

    Ok((largest_acked, ranges))
}

/// Decode ACK ranges into acknowledged packet numbers.
pub fn decode_ack_ranges(largest_acked: u64, ranges: &[(u64, u64)]) -> AtpResult<BTreeSet<u64>> {
    if ranges.is_empty() {
        return Err(AtpError::protocol("ACK must have at least one range"));
    }

    let mut result = BTreeSet::new();
    let mut current = largest_acked;

    for (idx, &(gap, range_len)) in ranges.iter().enumerate() {
        if idx > 0 {
            current = current.saturating_sub(gap + 1);
        } else if gap != 0 {
            return Err(AtpError::protocol("first ACK range gap must be 0"));
        }

        for offset in 0..=range_len {
            result.insert(current.saturating_sub(offset));
        }

        if range_len > 0 {
            current = current.saturating_sub(range_len);
        }
    }

    Ok(result)
}

pub fn validate_ack_ranges(largest_acked: u64, ranges: &[(u64, u64)]) -> AtpResult<()> {
    if ranges.is_empty() {
        return Err(AtpError::protocol("ACK ranges empty"));
    }
    if ranges.len() > MAX_ACK_RANGES {
        return Err(AtpError::protocol("too many ACK ranges"));
    }
    let _ = decode_ack_ranges(largest_acked, ranges)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(set: &[u64]) -> BTreeSet<u64> {
        let received: BTreeSet<u64> = set.iter().copied().collect();
        let (largest, ranges) = encode_ack_ranges(&received).unwrap();
        decode_ack_ranges(largest, &ranges).unwrap()
    }

    #[test]
    fn test_single_packet() {
        assert_eq!(roundtrip(&[1]), [1].into_iter().collect());
    }

    #[test]
    fn test_contiguous() {
        assert_eq!(roundtrip(&[1, 2, 3]), [1, 2, 3].into_iter().collect());
    }

    #[test]
    fn test_sparse() {
        assert_eq!(roundtrip(&[1, 3, 5]), [1, 3, 5].into_iter().collect());
    }

    #[test]
    fn test_mixed() {
        let input: Vec<u64> = vec![1, 2, 3, 5, 6, 8];
        assert_eq!(roundtrip(&input), input.into_iter().collect());
    }

    #[test]
    fn test_two_ranges() {
        let mut expected = BTreeSet::new();
        expected.extend(1..=10);
        expected.extend(20..=30);
        let input: Vec<u64> = expected.iter().copied().collect();
        assert_eq!(roundtrip(&input), expected);
    }
}
