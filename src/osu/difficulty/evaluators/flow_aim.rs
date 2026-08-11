use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::{
        evaluators::snap_aim::SnapAimEvaluator,
        object::OsuDifficultyObject,
    },
    util::{difficulty::smootherstep, float_ext::FloatExt},
};

pub struct FlowAimEvaluator;

impl FlowAimEvaluator {
    const VELOCITY_CHANGE_MULTIPLIER: f64 = 0.52;

    #[expect(clippy::too_many_lines, reason = "staying in-sync with lazer")]
    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        diff_objects: &'a [OsuDifficultyObject<'a>],
        with_slider_travel_dist: bool,
    ) -> f64 {
        let osu_curr_obj = curr;

        let Some(osu_last_obj) = curr.previous(0, diff_objects) else {
            return 0.0;
        };

        if curr.base.is_spinner() || curr.idx <= 1 || osu_last_obj.base.is_spinner() {
            return 0.0;
        }

        let osu_last_last_obj = curr.previous(1, diff_objects);

        let curr_distance = if with_slider_travel_dist {
            osu_curr_obj.lazy_jump_dist
        } else {
            osu_curr_obj.jump_dist
        };

        let prev_distance = if with_slider_travel_dist {
            osu_last_obj.lazy_jump_dist
        } else {
            osu_last_obj.jump_dist
        };

        let mut curr_velocity = curr_distance / osu_curr_obj.adjusted_delta_time;

        if osu_last_obj.base.is_slider() && with_slider_travel_dist {
            let slider_distance = osu_last_obj.lazy_travel_dist + osu_curr_obj.lazy_jump_dist;
            curr_velocity = curr_velocity.max(slider_distance / osu_curr_obj.adjusted_delta_time);
        }

        let prev_velocity = prev_distance / osu_last_obj.adjusted_delta_time;

        let mut flow_difficulty = curr_velocity;

        // Apply high circle size bonus to the base velocity
        flow_difficulty *= osu_curr_obj.small_circle_bonus.sqrt();

        // Rhythm changes are harder to flow
        let max_delta = osu_curr_obj
            .adjusted_delta_time
            .max(osu_last_obj.adjusted_delta_time);
        let min_delta = osu_curr_obj
            .adjusted_delta_time
            .min(osu_last_obj.adjusted_delta_time);
        flow_difficulty *= 1.0 + 0.25_f64.min(((max_delta - min_delta) / 50.0).powf(4.0));

        // Angular velocity bonus
        if let (Some(curr_angle), Some(last_angle)) = (osu_curr_obj.angle, osu_last_obj.angle) {
            let angle_difference = (curr_angle - last_angle).abs();
            let angle_difference_adjusted = (angle_difference / 2.0).sin() * 180.0;
            let angular_velocity =
                angle_difference_adjusted / (osu_curr_obj.adjusted_delta_time * 0.1);

            // Low angular velocity flow is easier to follow than erratic flow
            flow_difficulty *= 0.8 + (angular_velocity / 270.0).sqrt();
        }

        // Overlap factor
        let mut overlapped_notes_weight = 1.0;

        if curr.idx > 2 {
            let o1 = Self::calculate_overlap_factor(osu_curr_obj, osu_last_obj);
            let o2 = osu_last_last_obj
                .map(|last_last| Self::calculate_overlap_factor(osu_curr_obj, last_last))
                .unwrap_or(0.0);
            let o3 = osu_last_last_obj
                .map(|last_last| Self::calculate_overlap_factor(osu_last_obj, last_last))
                .unwrap_or(0.0);

            overlapped_notes_weight = 1.0 - o1 * o2 * o3;
        }

        // Acute angle bonus for flow
        if let Some(curr_angle) = osu_curr_obj.angle {
            flow_difficulty += curr_velocity
                * SnapAimEvaluator::calc_angle_acuteness(curr_angle)
                * overlapped_notes_weight;
        }

        // Velocity change bonus
        if prev_velocity.max(curr_velocity).not_eq(0.0) {
            let curr_vel = if with_slider_travel_dist {
                curr_distance / osu_curr_obj.adjusted_delta_time
            } else {
                curr_velocity
            };

            let dist_ratio = crate::util::difficulty::smoothstep(
                (prev_velocity - curr_vel).abs() / prev_velocity.max(curr_vel),
                0.0,
                1.0,
            );

            let overlap_vel_buff = (f64::from(OsuDifficultyObject::NORMALIZED_DIAMETER) * 1.25
                / osu_curr_obj
                    .adjusted_delta_time
                    .min(osu_last_obj.adjusted_delta_time))
            .min((prev_velocity - curr_vel).abs());

            flow_difficulty += overlap_vel_buff
                * dist_ratio
                * overlapped_notes_weight
                * Self::VELOCITY_CHANGE_MULTIPLIER;
        }

        // Slider velocity bonus
        if osu_curr_obj.base.is_slider() && with_slider_travel_dist {
            flow_difficulty += osu_curr_obj.travel_dist / osu_curr_obj.travel_time;
        }

        // Final velocity raised to a power
        flow_difficulty = flow_difficulty.powf(1.45);

        // Reduce difficulty for low spacing
        flow_difficulty
            * smootherstep(curr_distance, 0.0, f64::from(OsuDifficultyObject::NORMALIZED_RADIUS))
    }

    fn calculate_overlap_factor(first: &OsuDifficultyObject, second: &OsuDifficultyObject) -> f64 {
        let object_radius = first.object_radius;
        let distance = (first.base.stacked_pos() - second.base.stacked_pos()).length() as f64;
        (1.0 - ((distance - object_radius).max(0.0) / object_radius).powf(2.0)).clamp(0.0, 1.0)
    }
}
