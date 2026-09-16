//! Even-split child sling counts onto parent endpoints.
//!
//! Roadmap: Step 0 foundation (legacy layer engine).

/// Even-split child sling counts onto parent endpoints.
/// Remainder goes to the outer ends first.
///
/// Example: 4 slings onto 2 ends → `[2, 2]`; 5 onto 2 → `[3, 2]`.
pub fn split_slings(parent_ends: u32, child_count: u32) -> Vec<u32> {
    let ends = parent_ends.max(1) as usize;
    let child = child_count.max(1);
    let base = child / ends as u32;
    let rem = child % ends as u32;
    let mut out = vec![base; ends];
    // Distribute remainder to outer ends (first, then last, then inward).
    let mut left = 0usize;
    let mut right = ends.saturating_sub(1);
    let mut r = rem;
    let mut from_left = true;
    while r > 0 && left <= right {
        if from_left {
            out[left] += 1;
            if left == right {
                break;
            }
            left += 1;
        } else {
            out[right] += 1;
            if right == 0 {
                break;
            }
            right -= 1;
        }
        from_left = !from_left;
        r -= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_four_onto_two() {
        assert_eq!(split_slings(2, 4), vec![2, 2]);
    }

    #[test]
    fn split_five_onto_two_outer_bias() {
        assert_eq!(split_slings(2, 5), vec![3, 2]);
    }

    #[test]
    fn split_three_onto_two() {
        assert_eq!(split_slings(2, 3), vec![2, 1]);
    }
}
