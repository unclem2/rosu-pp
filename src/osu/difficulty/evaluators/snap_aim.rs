use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::object::OsuDifficultyObject,
    util::{
        difficulty::{milliseconds_to_bpm, reverse_lerp, smootherstep, smoothstep},
        float_ext::FloatExt,
    },
};

pub struct SnapAimEvaluator;

impl SnapAimEvaluator {
    const WIDE_ANGLE_MULTIPLIER: f64 = 9.67;
    const ACUTE_ANGLE_MULTIPLIER: f64 = 2.41;
    const SLIDER_MULTIPLIER: f64 = 1.5;
    const VELOCITY_CHANGE_MULTIPLIER: f64 = 0.9;
    const WIGGLE_MULTIPLIER: f64 = 1.02;

    #[expect(clippy::too_many_lines, reason = "staying in-sync with lazer")]
    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        diff_objects: &'a [OsuDifficultyObject<'a>],
        with_slider_travel_dist: bool,
    ) -> f64 {
        let osu_curr_obj = curr;

        let Some((_osu_last_last_obj, osu_last_obj)) = curr
            .previous(1, diff_objects)
            .zip(curr.previous(0, diff_objects))
            .filter(|(_, last)| !(curr.base.is_spinner() || last.base.is_spinner()))
        else {
            return 0.0;
        };

        let radius = OsuDifficultyObject::NORMALIZED_RADIUS as f64;
        let diameter = OsuDifficultyObject::NORMALIZED_DIAMETER as f64;

        let mut curr_distance = if with_slider_travel_dist {
            osu_curr_obj.lazy_jump_dist
        } else {
            osu_curr_obj.jump_dist
        };

        let mut curr_velocity = curr_distance / osu_curr_obj.adjusted_delta_time;

        if osu_last_obj.base.is_slider() && with_slider_travel_dist {
            let slider_distance = osu_last_obj.lazy_travel_dist + osu_curr_obj.lazy_jump_dist;
            curr_velocity = curr_velocity.max(slider_distance / osu_curr_obj.adjusted_delta_time);
        }

        let prev_distance = if with_slider_travel_dist {
            osu_last_obj.lazy_jump_dist
        } else {
            osu_last_obj.jump_dist
        };

        let prev_velocity = prev_distance / osu_last_obj.adjusted_delta_time;

        let mut snap_difficulty = curr_velocity;

        snap_difficulty *= Self::vector_angle_repetition(osu_curr_obj, osu_last_obj, diff_objects);

        #[expect(unused_assignments, reason = "staying in-sync with lazer")]
        let mut wide_angle_bonus = 0.0;
        let mut acute_angle_bonus = 0.0;

        if let (Some(curr_angle), Some(last_angle)) = (osu_curr_obj.angle, osu_last_obj.angle) {
            let velocity_influence = curr_velocity.min(prev_velocity);

            if osu_curr_obj
                .adjusted_delta_time
                .max(osu_last_obj.adjusted_delta_time)
                < 1.25
                    * osu_curr_obj
                        .adjusted_delta_time
                        .min(osu_last_obj.adjusted_delta_time)
            {
                acute_angle_bonus = Self::calc_angle_acuteness(curr_angle);
                acute_angle_bonus *= 0.08
                    + 0.92
                        * (1.0
                            - acute_angle_bonus
                                .min(Self::calc_angle_acuteness(last_angle).powf(3.0)));

                acute_angle_bonus *= velocity_influence
                    * smootherstep(
                        milliseconds_to_bpm(osu_curr_obj.adjusted_delta_time, Some(2)),
                        300.0,
                        400.0,
                    )
                    * smootherstep(curr_distance, 0.0, diameter * 2.0);
            }

            wide_angle_bonus = Self::calc_angle_wideness(curr_angle);
            wide_angle_bonus *= 0.25
                + 0.75
                    * (1.0
                        - wide_angle_bonus
                            .min(Self::calc_angle_wideness(last_angle).powf(3.0)));

            let wide_angle_time_scale = 1.45;
            let mut wide_angle_curr_velocity =
                curr_distance / osu_curr_obj.adjusted_delta_time.powf(wide_angle_time_scale);
            let wide_angle_prev_velocity =
                prev_distance / osu_last_obj.adjusted_delta_time.powf(wide_angle_time_scale);

            if osu_last_obj.base.is_slider() && with_slider_travel_dist {
                let slider_distance = osu_last_obj.lazy_travel_dist + osu_curr_obj.lazy_jump_dist;
                wide_angle_curr_velocity = wide_angle_curr_velocity.max(
                    slider_distance / osu_curr_obj.adjusted_delta_time.powf(wide_angle_time_scale),
                );
            }

            wide_angle_bonus *= wide_angle_curr_velocity.min(wide_angle_prev_velocity);

            if let Some(osu_last_2_obj) = curr.previous(2, diff_objects) {
                let distance =
                    (osu_last_2_obj.base.stacked_pos() - osu_last_obj.base.stacked_pos()).length() as f64;

                if distance < 1.0 {
                    wide_angle_bonus *= 1.0 - 0.55 * (1.0 - distance);
                }
            }

            snap_difficulty += (acute_angle_bonus * Self::ACUTE_ANGLE_MULTIPLIER)
                .max(wide_angle_bonus * Self::WIDE_ANGLE_MULTIPLIER);

            let wiggle_bonus = velocity_influence
                * smootherstep(curr_distance, radius, diameter)
                * reverse_lerp(curr_distance, diameter * 3.0, diameter).powf(1.8)
                * smootherstep(curr_angle, f64::to_radians(110.0), f64::to_radians(60.0))
                * smootherstep(prev_distance, radius, diameter)
                * reverse_lerp(prev_distance, diameter * 3.0, diameter).powf(1.8)
                * smootherstep(last_angle, f64::to_radians(110.0), f64::to_radians(60.0));

            snap_difficulty += wiggle_bonus * Self::WIGGLE_MULTIPLIER;
        }

        if prev_velocity.max(curr_velocity).not_eq(0.0) {
            if with_slider_travel_dist {
                curr_distance = osu_curr_obj.lazy_jump_dist;
            }

            let curr_vel_for_change = curr_distance / osu_curr_obj.adjusted_delta_time;

            let dist_ratio = smoothstep(
                (prev_velocity - curr_vel_for_change).abs()
                    / prev_velocity.max(curr_vel_for_change),
                0.0,
                1.0,
            );

            let overlap_vel_buff = (diameter * 1.25
                / osu_curr_obj
                    .adjusted_delta_time
                    .min(osu_last_obj.adjusted_delta_time))
            .min((prev_velocity - curr_vel_for_change).abs());

            let mut velocity_change_bonus = overlap_vel_buff * dist_ratio;
            velocity_change_bonus *= (osu_curr_obj.adjusted_delta_time
                .min(osu_last_obj.adjusted_delta_time)
                / osu_curr_obj
                    .adjusted_delta_time
                    .max(osu_last_obj.adjusted_delta_time))
            .powf(2.0);

            snap_difficulty += velocity_change_bonus * Self::VELOCITY_CHANGE_MULTIPLIER;
        }

        if osu_curr_obj.base.is_slider() && with_slider_travel_dist {
            let slider_bonus = osu_curr_obj.travel_dist / osu_curr_obj.travel_time;
            snap_difficulty += if slider_bonus < 1.0 {
                slider_bonus
            } else {
                slider_bonus.powf(0.75)
            } * Self::SLIDER_MULTIPLIER;
        }

        snap_difficulty *= osu_curr_obj.small_circle_bonus;
        snap_difficulty *= Self::high_bpm_bonus(osu_curr_obj.adjusted_delta_time);

        snap_difficulty
    }

    fn vector_angle_repetition<'a>(
        current: &'a OsuDifficultyObject<'a>,
        previous: &'a OsuDifficultyObject<'a>,
        diff_objects: &'a [OsuDifficultyObject<'a>],
    ) -> f64 {
        let _ = previous;

        let (Some(curr_norm_angle), Some(_prev_norm_angle)) =
            (current.normalised_vector_angle, previous.normalised_vector_angle)
        else {
            return 1.0;
        };

        const NOTE_LIMIT: usize = 6;
        const MAXIMUM_REPETITION_NERF: f64 = 0.15;
        const MAXIMUM_VECTOR_INFLUENCE: f64 = 0.5;

        let mut constant_angle_count = 0.0;

        for index in 0..NOTE_LIMIT {
            let Some(prev_obj) = current.previous(index, diff_objects) else {
                break;
            };

            if current
                .adjusted_delta_time
                .max(prev_obj.adjusted_delta_time)
                > 1.1 * current.adjusted_delta_time.min(prev_obj.adjusted_delta_time)
            {
                break;
            }

            if let Some(prev_obj_norm_angle) = prev_obj.normalised_vector_angle {
                let angle_difference = (curr_norm_angle - prev_obj_norm_angle).abs();
                constant_angle_count +=
                    (8.0 * f64::min(f64::to_radians(11.25), angle_difference)).cos();
            }
        }

        let vector_repetition = (f64::min(0.5 / constant_angle_count, 1.0)).powf(2.0);
        let stack_factor = smootherstep(
            current.lazy_jump_dist,
            0.0,
            OsuDifficultyObject::NORMALIZED_DIAMETER as f64,
        );
        let curr_angle = current.angle.unwrap_or(0.0);
        let last_angle = previous.angle.unwrap_or(0.0);
        let angle_difference_adjusted =
            (2.0 * f64::min(f64::to_radians(45.0), (curr_angle - last_angle).abs() * stack_factor))
                .cos();
        let base_nerf = 1.0
            - MAXIMUM_REPETITION_NERF
                * Self::calc_angle_acuteness(last_angle)
                * angle_difference_adjusted;

        (base_nerf
            + (1.0 - base_nerf)
                * vector_repetition
                * MAXIMUM_VECTOR_INFLUENCE
                * stack_factor)
            .powf(2.0)
    }

    pub fn calc_angle_wideness(angle: f64) -> f64 {
        smoothstep(angle, f64::to_radians(40.0), f64::to_radians(140.0))
    }

    pub fn calc_angle_acuteness(angle: f64) -> f64 {
        smoothstep(angle, f64::to_radians(140.0), f64::to_radians(40.0))
    }

    fn high_bpm_bonus(ms: f64) -> f64 {
        1.0 / (1.0 - 0.03_f64.powf((ms / 1000.0).powf(0.65)))
    }
}
