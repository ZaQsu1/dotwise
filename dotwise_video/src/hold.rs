pub fn apply(
    src: &[u8],
    sw: usize,
    sh: usize,
    out: &mut [u8],
    ow: usize,
    oh: usize,
    reference: &mut [u8],
    prev_out: &[u8],
    threshold: f32,
) {
    let limit = (threshold * 255.0).round() as i32;
    let mut changed = vec![false; sw * sh];
    for i in 0..sw * sh {
        let d = (0..3).map(|c| (src[i * 3 + c] as i32 - reference[i * 3 + c] as i32).abs()).max().unwrap_or(0);
        if d > limit {
            changed[i] = true;
            reference[i * 3..i * 3 + 3].copy_from_slice(&src[i * 3..i * 3 + 3]);
        }
    }
    if prev_out.len() != out.len() { return; }
    for oy in 0..oh {
        let sy = (oy * sh / oh.max(1)).min(sh - 1);
        for ox in 0..ow {
            let sx = (ox * sw / ow.max(1)).min(sw - 1);
            if !changed[sy * sw + sx] {
                let o = (oy * ow + ox) * 3;
                out[o..o + 3].copy_from_slice(&prev_out[o..o + 3]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_pixels_keep_the_previous_output() {
        let src = vec![10u8; 4 * 4 * 3];
        let mut reference = src.clone();
        let prev = vec![0u8; 4 * 4 * 3];
        let mut out = vec![255u8; 4 * 4 * 3];
        apply(&src, 4, 4, &mut out, 4, 4, &mut reference, &prev, 0.02);
        assert!(out.iter().all(|v| *v == 0));
    }

    #[test]
    fn changed_pixels_use_the_new_output_and_update_the_reference() {
        let mut src = vec![10u8; 4 * 4 * 3];
        let mut reference = src.clone();
        src[5 * 3] = 200;
        let prev = vec![0u8; 4 * 4 * 3];
        let mut out = vec![255u8; 4 * 4 * 3];
        apply(&src, 4, 4, &mut out, 4, 4, &mut reference, &prev, 0.02);
        assert_eq!(out[5 * 3], 255);
        assert_eq!(out[0], 0);
        assert_eq!(reference[5 * 3], 200);
    }

    #[test]
    fn slow_drift_is_eventually_picked_up() {
        let mut reference = vec![10u8; 3];
        let prev = vec![0u8; 3];
        let mut recomputed = false;
        for step in 1..=20u8 {
            let src = vec![10 + step * 2; 3];
            let mut out = vec![255u8; 3];
            apply(&src, 1, 1, &mut out, 1, 1, &mut reference, &prev, 0.05);
            if out[0] == 255 { recomputed = true; }
        }
        assert!(recomputed);
    }

    #[test]
    fn works_when_the_output_is_larger_than_the_source() {
        let src = vec![50u8; 2 * 2 * 3];
        let mut reference = src.clone();
        let prev = vec![7u8; 4 * 4 * 3];
        let mut out = vec![99u8; 4 * 4 * 3];
        apply(&src, 2, 2, &mut out, 4, 4, &mut reference, &prev, 0.02);
        assert!(out.iter().all(|v| *v == 7));
    }
}
