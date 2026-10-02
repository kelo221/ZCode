//! Tests for mouse wheel button & continuous autoscrolling.

#[cfg(test)]
mod tests {
    use crate::transcript::scroll::{
        AUTOSCROLL_DEADZONE, MiddleScrollState, autoscroll_speed, clamp_scroll_distance,
        follows_after_scroll,
    };
    use gpui::{point, px};

    #[test]
    fn test_autoscroll_deadzone_positive() {
        const { assert!(AUTOSCROLL_DEADZONE > 0.0) };
        const { assert!(AUTOSCROLL_DEADZONE <= 20.0) };
    }

    #[test]
    fn test_autoscroll_deadzone_returns_zero() {
        assert_eq!(autoscroll_speed(0.0), 0.0);
        assert_eq!(autoscroll_speed(AUTOSCROLL_DEADZONE * 0.5), 0.0);
        assert_eq!(autoscroll_speed(-AUTOSCROLL_DEADZONE * 0.5), 0.0);
        assert_eq!(autoscroll_speed(AUTOSCROLL_DEADZONE), 0.0);
        assert_eq!(autoscroll_speed(-AUTOSCROLL_DEADZONE), 0.0);
    }

    #[test]
    fn test_autoscroll_speed_direction_and_symmetry() {
        let speed_down = autoscroll_speed(25.0);
        let speed_up = autoscroll_speed(-25.0);
        assert!(speed_down > 0.0);
        assert!(speed_up < 0.0);
        assert!((speed_down + speed_up).abs() < 1e-6);
    }

    #[test]
    fn test_autoscroll_speed_monotonic_and_capped() {
        let s1 = autoscroll_speed(20.0);
        let s2 = autoscroll_speed(40.0);
        let s3 = autoscroll_speed(100.0);
        assert!(s1 < s2);
        assert!(s2 < s3);

        // Maximum velocity capped at 28.0 px/frame
        assert_eq!(autoscroll_speed(1000.0), 28.0);
        assert_eq!(autoscroll_speed(-1000.0), -28.0);
    }

    #[test]
    fn test_clamp_scroll_distance_downward() {
        // Within bounds: full distance allowed
        assert_eq!(
            clamp_scroll_distance(px(50.0), px(200.0), px(20.0)),
            px(20.0)
        );
        // Near bottom: clamped to remaining distance
        assert_eq!(
            clamp_scroll_distance(px(190.0), px(200.0), px(20.0)),
            px(10.0)
        );
        // Exactly at bottom: zero allowed
        assert_eq!(
            clamp_scroll_distance(px(200.0), px(200.0), px(20.0)),
            px(0.0)
        );
        // Past bottom: zero allowed
        assert_eq!(
            clamp_scroll_distance(px(210.0), px(200.0), px(20.0)),
            px(0.0)
        );
        // Content fits in viewport (max <= 0): zero allowed
        assert_eq!(clamp_scroll_distance(px(0.0), px(0.0), px(20.0)), px(0.0));
    }

    #[test]
    fn test_clamp_scroll_distance_upward() {
        // Within bounds: full upward delta allowed
        assert_eq!(
            clamp_scroll_distance(px(50.0), px(200.0), px(-20.0)),
            px(-20.0)
        );
        // Near top: clamped to remaining distance to 0
        assert_eq!(
            clamp_scroll_distance(px(10.0), px(200.0), px(-20.0)),
            px(-10.0)
        );
        // Exactly at top: zero allowed
        assert_eq!(
            clamp_scroll_distance(px(0.0), px(200.0), px(-20.0)),
            px(0.0)
        );
        // Past top: zero allowed
        assert_eq!(
            clamp_scroll_distance(px(-5.0), px(200.0), px(-20.0)),
            px(0.0)
        );
    }

    #[test]
    fn test_clamp_scroll_distance_zero() {
        assert_eq!(clamp_scroll_distance(px(50.0), px(200.0), px(0.0)), px(0.0));
    }

    #[test]
    fn test_clamp_scroll_distance_rejects_nan() {
        // NaN must not fall through to the upward branch and snap to the top.
        assert_eq!(clamp_scroll_distance(px(50.0), px(200.0), px(f32::NAN)), px(0.0));
        assert_eq!(clamp_scroll_distance(px(50.0), px(200.0), px(f32::INFINITY)), px(0.0));
    }

    #[test]
    fn test_clamp_scroll_distance_after_content_shrinks() {
        // Offset beyond a shrunken max: downward is a no-op, upward still works.
        assert_eq!(clamp_scroll_distance(px(300.0), px(200.0), px(20.0)), px(0.0));
        assert_eq!(clamp_scroll_distance(px(300.0), px(200.0), px(-20.0)), px(-20.0));
        // Empty / fitting list in both directions.
        assert_eq!(clamp_scroll_distance(px(0.0), px(0.0), px(-20.0)), px(0.0));
    }

    #[test]
    fn test_follow_flag_after_programmatic_scroll() {
        // Any upward step detaches from the tail.
        assert_eq!(follows_after_scroll(px(200.0), px(200.0), px(-1.0)), Some(false));
        // Downward re-attaches only on reaching the bottom.
        assert_eq!(follows_after_scroll(px(100.0), px(200.0), px(20.0)), Some(false));
        assert_eq!(follows_after_scroll(px(190.0), px(200.0), px(10.0)), Some(true));
        // No movement leaves the flag untouched.
        assert_eq!(follows_after_scroll(px(200.0), px(200.0), px(0.0)), None);
    }

    #[test]
    fn test_middle_scroll_state_initialization() {
        let p = point(px(100.0), px(200.0));
        let ms = MiddleScrollState {
            anchor: p,
            last_pos: p,
            button_held: true,
        };
        assert_eq!(ms.anchor, ms.last_pos);
        assert!(ms.button_held);
    }
}
