use std::fmt::Debug;

use polars_utils::IdxSize;
use polars_utils::itertools::Itertools;
use polars_utils::total_ord::TotalEq;

use crate::types::NativeType;

/// Find partition indexes such that every partition contains unique groups.
fn find_partition_points<T>(values: &[T], n: usize, descending: bool) -> Vec<usize>
where
    T: Debug + NativeType,
{
    let len = values.len();
    if n > len {
        return find_partition_points(values, len / 2, descending);
    }
    if n < 2 {
        return vec![];
    }
    let chunk_size = len / n;

    let mut partition_points = Vec::with_capacity(n + 1);

    let mut start_idx = 0;
    loop {
        let end_idx = start_idx + chunk_size;
        if end_idx >= len {
            break;
        }
        // first take that partition as a slice
        // and then find the location where the group of the latest value starts
        let part = &values[start_idx..end_idx];

        let latest_val = values[end_idx];
        let idx = if descending {
            part.partition_point(|v| v.tot_gt(&latest_val))
        } else {
            part.partition_point(|v| v.tot_lt(&latest_val))
        };

        if idx != 0 {
            partition_points.push(idx + start_idx)
        }

        start_idx += chunk_size;
    }
    partition_points
}

pub fn create_clean_partitions<T>(values: &[T], n: usize, descending: bool) -> Vec<&[T]>
where
    T: Debug + NativeType,
{
    let part_idx = find_partition_points(values, n, descending);
    let mut out = Vec::with_capacity(n + 1);

    let mut start_idx = 0_usize;
    for end_idx in part_idx {
        if end_idx != start_idx {
            out.push(&values[start_idx..end_idx]);
            start_idx = end_idx;
        }
    }
    let latest = &values[start_idx..];
    if !latest.is_empty() {
        out.push(latest)
    }

    out
}

pub fn partition_to_groups_amortized_varsize<T, I>(
    values: I,
    values_len: IdxSize,
    first_group_offset: IdxSize,
    nulls_first: bool,
    offset: IdxSize,
    out: &mut Vec<[IdxSize; 2]>,
) where
    T: Debug + TotalEq,
    I: IntoIterator<Item = T>,
{
    let mut values = values.into_iter().enumerate_idx();
    if let Some((i, mut first)) = values.next() {
        out.clear();
        if nulls_first && first_group_offset > 0 {
            out.push([0, first_group_offset])
        }

        let mut first_idx = if nulls_first { first_group_offset } else { 0 } + offset;
        let mut start = i;

        for (i, val) in values {
            // new group reached
            if val.tot_ne(&first) {
                let len = i - start;
                start = i;
                out.push([first_idx, len]);
                first_idx += len;
                first = val;
            }
        }
        // add last group
        if nulls_first {
            out.push([first_idx, values_len + first_group_offset - first_idx]);
        } else {
            out.push([first_idx, values_len - (first_idx - offset)]);
        }

        if !nulls_first && first_group_offset > 0 {
            out.push([values_len + offset, first_group_offset])
        }
    }
}

pub fn partition_to_groups_amortized<T>(
    values: &[T],
    first_group_offset: IdxSize,
    nulls_first: bool,
    offset: IdxSize,
    out: &mut Vec<[IdxSize; 2]>,
) where
    T: Debug + TotalEq + Sized,
{
    if let Some(mut first) = values.first() {
        out.clear();
        if nulls_first && first_group_offset > 0 {
            out.push([0, first_group_offset])
        }

        let mut first_idx = if nulls_first { first_group_offset } else { 0 } + offset;

        for val in values {
            // new group reached
            if val.tot_ne(first) {
                let val_ptr = val as *const T;
                let first_ptr = first as *const T;

                // SAFETY:
                // all pointers suffice the invariants
                let len = unsafe { val_ptr.offset_from(first_ptr) } as IdxSize;
                out.push([first_idx, len]);
                first_idx += len;
                first = val;
            }
        }
        // add last group
        if nulls_first {
            out.push([
                first_idx,
                values.len() as IdxSize + first_group_offset - first_idx,
            ]);
        } else {
            out.push([first_idx, values.len() as IdxSize - (first_idx - offset)]);
        }

        if !nulls_first && first_group_offset > 0 {
            out.push([values.len() as IdxSize + offset, first_group_offset])
        }
    }
}

/// Take a clean-partitioned slice and return the groups slices
/// With clean-partitioned we mean that the slice contains all groups and are not spilled to another partition.
///
/// `first_group_offset` can be used to add insert the `null` values group.
pub fn partition_to_groups<T>(
    values: &[T],
    first_group_offset: IdxSize,
    nulls_first: bool,
    offset: IdxSize,
) -> Vec<[IdxSize; 2]>
where
    T: Debug + NativeType,
{
    if values.is_empty() {
        return vec![];
    }
    let mut out = Vec::with_capacity(values.len() / 10);
    partition_to_groups_amortized(values, first_group_offset, nulls_first, offset, &mut out);
    out
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_partition_points() {
        let values = &[1, 3, 3, 3, 3, 5, 5, 5, 9, 9, 10];

        assert_eq!(find_partition_points(values, 4, false), &[1, 5, 8, 10]);
        assert_eq!(
            partition_to_groups(values, 0, true, 0),
            &[[0, 1], [1, 4], [5, 3], [8, 2], [10, 1]]
        );
        assert_eq!(
            partition_to_groups(values, 5, true, 0),
            &[[0, 5], [5, 1], [6, 4], [10, 3], [13, 2], [15, 1]]
        );
        assert_eq!(
            partition_to_groups(values, 5, false, 0),
            &[[0, 1], [1, 4], [5, 3], [8, 2], [10, 1], [11, 5]]
        );
    }
}
