#![allow(dead_code)]

/// Column widths that fill `available`. `natural` = unwrapped content width per column,
/// `min` = longest-word width per column. Both slices have one entry per column.
pub fn column_widths(natural: &[f32], min: &[f32], available: f32) -> Vec<f32> {
    debug_assert_eq!(natural.len(), min.len());
    let n = natural.len();
    if n == 0 {
        return Vec::new();
    }
    let sum_nat: f32 = natural.iter().sum();
    if sum_nat <= available {
        // Fits: hand out the spare space in proportion to content (equally if all empty).
        return if sum_nat > 0.0 {
            natural.iter().map(|w| w * available / sum_nat).collect()
        } else {
            vec![available / n as f32; n]
        };
    }
    if min.iter().sum::<f32>() >= available {
        return min.to_vec();
    }
    // Water-fill: lower a common cap until the capped widths sum to `available`.
    let capped = |cap: f32| {
        natural
            .iter()
            .zip(min)
            .map(move |(&w, &m)| w.min(cap).max(m))
    };
    let (mut lo, mut hi) = (0.0f32, natural.iter().cloned().fold(0.0, f32::max));
    for _ in 0..64 {
        let mid = (lo + hi) / 2.0;
        if capped(mid).sum::<f32>() > available {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    capped(lo).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(v: &[f32]) -> f32 {
        v.iter().sum()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn fits_scales_up_proportionally() {
        let w = column_widths(&[100.0, 300.0], &[10.0, 10.0], 800.0);
        assert!(close(w[0], 200.0) && close(w[1], 600.0), "{w:?}");
    }

    #[test]
    fn overflow_shrinks_widest_first() {
        let w = column_widths(&[50.0, 500.0, 400.0], &[20.0, 40.0, 40.0], 650.0);
        // cap = 300: 50 + 300 + 300
        assert!(
            close(w[0], 50.0) && close(w[1], 300.0) && close(w[2], 300.0),
            "{w:?}"
        );
        assert!(close(sum(&w), 650.0));
    }

    #[test]
    fn overflow_respects_min() {
        let w = column_widths(&[500.0, 500.0], &[350.0, 10.0], 600.0);
        assert!(close(w[0], 350.0) && close(w[1], 250.0), "{w:?}");
    }

    #[test]
    fn all_min_when_min_exceeds_available() {
        let w = column_widths(&[500.0, 500.0], &[300.0, 300.0], 400.0);
        assert_eq!(w, vec![300.0, 300.0]);
    }

    #[test]
    fn single_column() {
        assert!(close(column_widths(&[100.0], &[10.0], 400.0)[0], 400.0));
        assert!(close(column_widths(&[1000.0], &[10.0], 400.0)[0], 400.0));
    }

    #[test]
    fn empty_and_zero_width() {
        assert!(column_widths(&[], &[], 100.0).is_empty());
        assert_eq!(
            column_widths(&[0.0, 0.0], &[0.0, 0.0], 100.0),
            vec![50.0, 50.0]
        );
        let w = column_widths(&[0.0, 100.0], &[0.0, 10.0], 300.0);
        assert!(close(w[0], 0.0) && close(w[1], 300.0));
    }

    #[test]
    fn sum_invariant() {
        let nat = [120.0, 30.0, 800.0, 0.0, 260.0];
        let min = [40.0, 30.0, 60.0, 0.0, 50.0];
        for avail in [180.0, 250.0, 400.0, 700.0, 1210.0, 2000.0] {
            let w = column_widths(&nat, &min, avail);
            assert!(close(sum(&w), avail), "avail {avail}: {w:?}");
            assert!(w.iter().zip(&min).all(|(a, m)| a >= m));
        }
    }
}
