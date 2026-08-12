use std::f64::consts::PI;

use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::object::OsuDifficultyObject,
    util::difficulty::{norm, reverse_lerp, smootherstep},
};

const READING_WINDOW_SIZE: f64 = 3000.0;
const DISTANCE_INFLUENCE_THRESHOLD: f64 = OsuDifficultyObject::NORMALIZED_DIAMETER as f64 * 1.5;

pub fn evaluate_diff_of(
    curr: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
    hidden: bool,
) -> f64 {
    if curr.base.is_spinner() || curr.idx == 0 {
        return 0.0;
    }

    let next_obj = curr.next(0, diff_objects);

    let velocity = (1.0_f64).max(curr.lazy_jump_dist / curr.adjusted_delta_time);

    let current_visible_object_density = retrieve_current_visible_object_density(curr, diff_objects);
    let past_object_difficulty_influence = get_past_object_difficulty_influence(curr, diff_objects);

    let constant_angle_nerf_factor = get_constant_angle_nerf_factor(curr, diff_objects);

    let note_density_difficulty = calculate_density_difficulty(
        next_obj,
        velocity,
        constant_angle_nerf_factor,
        past_object_difficulty_influence,
        current_visible_object_density,
    );

    let hidden_difficulty = if hidden {
        calculate_hidden_difficulty(
            curr,
            diff_objects,
            past_object_difficulty_influence,
            current_visible_object_density,
            velocity,
            constant_angle_nerf_factor,
        )
    } else {
        0.0
    };

    let preempt_difficulty =
        calculate_preempt_difficulty(velocity, constant_angle_nerf_factor, curr.preempt);

    let reading_difficulty =
        norm(1.5, [preempt_difficulty, hidden_difficulty, note_density_difficulty]);

    reading_difficulty * high_bpm_bonus(curr.adjusted_delta_time)
}

fn calculate_density_difficulty(
    next_obj: Option<&OsuDifficultyObject<'_>>,
    velocity: f64,
    constant_angle_nerf_factor: f64,
    past_object_difficulty_influence: f64,
    current_visible_object_density: f64,
) -> f64 {
    const DENSITY_MULTIPLIER: f64 = 2.4;
    const DENSITY_DIFFICULTY_BASE: f64 = 2.5;

    let mut future_object_difficulty_influence =
        current_visible_object_density.sqrt();

    if let Some(next) = next_obj {
        future_object_difficulty_influence *=
            smootherstep(next.lazy_jump_dist, 15.0, DISTANCE_INFLUENCE_THRESHOLD);
    }

    let note_density_difficulty = (past_object_difficulty_influence
        + future_object_difficulty_influence)
        .powf(1.7)
        * 0.4
        * constant_angle_nerf_factor
        * velocity;

    let note_density_difficulty = (note_density_difficulty - DENSITY_DIFFICULTY_BASE).max(0.0);

    note_density_difficulty.powf(0.45) * DENSITY_MULTIPLIER
}

fn calculate_preempt_difficulty(
    velocity: f64,
    constant_angle_nerf_factor: f64,
    preempt: f64,
) -> f64 {
    const PREEMPT_BALANCING_FACTOR: f64 = 140000.0;
    const PREEMPT_STARTING_POINT: f64 = 500.0;

    let diff = PREEMPT_STARTING_POINT - preempt + (preempt - PREEMPT_STARTING_POINT).abs();
    let preempt_difficulty = (diff / 2.0).powf(2.5) / PREEMPT_BALANCING_FACTOR;

    preempt_difficulty * constant_angle_nerf_factor * velocity
}

fn calculate_hidden_difficulty(
    curr: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
    past_object_difficulty_influence: f64,
    current_visible_object_density: f64,
    velocity: f64,
    constant_angle_nerf_factor: f64,
) -> f64 {
    const HIDDEN_MULTIPLIER: f64 = 0.28;

    let preempt_factor = curr.preempt.powf(2.2) * 0.01;

    let density_factor =
        (current_visible_object_density + past_object_difficulty_influence).powf(3.3) * 3.0;

    let mut hidden_difficulty =
        (preempt_factor + density_factor) * constant_angle_nerf_factor * velocity * 0.01;

    hidden_difficulty = hidden_difficulty.powf(0.4) * HIDDEN_MULTIPLIER;

    if let Some(previous_obj) = curr.previous(0, diff_objects) {
        if curr.lazy_jump_dist == 0.0
            && curr.opacity_at(
                previous_obj.base.start_time,
                true,
                curr.preempt,
                curr.preempt * crate::osu::difficulty::HD_FADE_IN_DURATION_MULTIPLIER,
            ) == 0.0
            && previous_obj.start_time > curr.start_time - curr.preempt
        {
            hidden_difficulty += HIDDEN_MULTIPLIER * 2500.0
                / curr.adjusted_delta_time.powf(1.5);
        }
    }

    hidden_difficulty
}

fn get_past_object_difficulty_influence(
    curr: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
) -> f64 {
    let mut past_object_difficulty_influence = 0.0;

    for loop_obj in retrieve_past_visible_objects(curr, diff_objects) {
        let mut loop_difficulty =
            curr.opacity_at(loop_obj.base.start_time, false, curr.preempt, curr.fade_in);

        loop_difficulty *=
            smootherstep(loop_obj.lazy_jump_dist, 15.0, DISTANCE_INFLUENCE_THRESHOLD);

        let time_between_curr_and_loop_obj = curr.start_time - loop_obj.start_time;
        let time_nerf_factor = get_time_nerf_factor(time_between_curr_and_loop_obj);

        loop_difficulty *= time_nerf_factor;
        past_object_difficulty_influence += loop_difficulty;
    }

    past_object_difficulty_influence
}

fn retrieve_past_visible_objects<'a>(
    current: &OsuDifficultyObject<'a>,
    diff_objects: &'a [OsuDifficultyObject<'a>],
) -> Vec<&'a OsuDifficultyObject<'a>> {
    let mut result = Vec::new();

    for i in 0..current.idx {
        let Some(hit_object) = current.previous(i, diff_objects) else {
            break;
        };

        if current.start_time - hit_object.start_time > READING_WINDOW_SIZE
            || hit_object.start_time < current.start_time - current.preempt
        {
            break;
        }

        result.push(hit_object);
    }

    result
}

fn retrieve_current_visible_object_density(
    current: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
) -> f64 {
    let mut visible_object_count = 0.0;

    let mut hit_object = current.next(0, diff_objects);

    while let Some(obj) = hit_object {
        if obj.start_time - current.start_time > READING_WINDOW_SIZE
            || current.start_time < obj.start_time - obj.preempt
        {
            break;
        }

        let time_between_curr_and_loop_obj = obj.start_time - current.start_time;
        let time_nerf_factor = get_time_nerf_factor(time_between_curr_and_loop_obj);

        visible_object_count +=
            obj.opacity_at(current.base.start_time, false, obj.preempt, obj.fade_in)
                * time_nerf_factor;

        hit_object = obj.next(0, diff_objects);
    }

    visible_object_count
}

fn get_constant_angle_nerf_factor(
    current: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
) -> f64 {
    const MINIMUM_ANGLE_RELEVANCY_TIME: f64 = 2000.0;
    const MAXIMUM_ANGLE_RELEVANCY_TIME: f64 = 200.0;

    let mut constant_angle_count = 0.0;
    let mut index = 0;
    let mut current_time_gap = 0.0;

    let mut loop_obj_prev0 = current;
    let mut loop_obj_prev1: Option<&OsuDifficultyObject<'_>> = None;
    let mut loop_obj_prev2: Option<&OsuDifficultyObject<'_>> = None;

    while current_time_gap < MINIMUM_ANGLE_RELEVANCY_TIME {
        let Some(loop_obj) = current.previous(index, diff_objects) else {
            break;
        };

        let long_interval_factor = 1.0
            - reverse_lerp(
                loop_obj.adjusted_delta_time,
                MAXIMUM_ANGLE_RELEVANCY_TIME,
                MINIMUM_ANGLE_RELEVANCY_TIME,
            );

        if let (Some(loop_obj_angle), Some(current_angle)) = (loop_obj.angle, current.angle) {
            let angle_difference = (current_angle - loop_obj_angle).abs();
            let mut angle_difference_alternating = PI;

            if let (Some(prev0_angle), Some(prev1), Some(prev2)) = (
                loop_obj_prev0.angle,
                loop_obj_prev1.and_then(|o| o.angle),
                loop_obj_prev2.and_then(|o| o.angle),
            ) {
                let mut alt_diff = (prev1 - loop_obj.angle.unwrap_or(0.0)).abs();
                alt_diff += (prev2 - prev0_angle).abs();

                let mut weight = 1.0;

                weight *= reverse_lerp(
                    loop_obj.angle.unwrap_or(0.0).min(prev0_angle) * 180.0 / PI,
                    20.0,
                    5.0,
                );
                weight *= reverse_lerp(
                    loop_obj.angle.unwrap_or(0.0).max(prev0_angle) * 180.0 / PI,
                    60.0,
                    120.0,
                );

                angle_difference_alternating =
                    PI * (1.0 - weight) + (0.1 * alt_diff) * weight;
            }

            let stack_factor = smootherstep(loop_obj.lazy_jump_dist, 0.0, OsuDifficultyObject::NORMALIZED_RADIUS as f64);

            constant_angle_count += (3.0
                * (30.0 * PI / 180.0)
                    .min(angle_difference.min(angle_difference_alternating) * stack_factor))
                .cos()
                * long_interval_factor;
        }

        current_time_gap = current.start_time - loop_obj.start_time;
        index += 1;

        loop_obj_prev2 = loop_obj_prev1;
        loop_obj_prev1 = Some(loop_obj_prev0);
        loop_obj_prev0 = loop_obj;
    }

    (2.0 / constant_angle_count).clamp(0.2, 1.0)
}

fn get_time_nerf_factor(delta_time: f64) -> f64 {
    (2.0 - delta_time / (READING_WINDOW_SIZE / 2.0)).clamp(0.0, 1.0)
}

fn high_bpm_bonus(ms: f64) -> f64 {
    1.0 / (1.0 - 0.8_f64.powf(ms / 1000.0))
}
