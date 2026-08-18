use crate::{
    osu::difficulty::{
        evaluators::{agility::AgilityEvaluator, flow_aim::FlowAimEvaluator, snap_aim::SnapAimEvaluator},
        object::OsuDifficultyObject,
    },
    util::difficulty::{logistic, norm},
};

pub struct AimEvaluator;

impl AimEvaluator {
    const SKILL_MULTIPLIER_SNAP: f64 = 70.9;
    const SKILL_MULTIPLIER_AGILITY: f64 = 2.35;
    const SKILL_MULTIPLIER_FLOW: f64 = 242.0;
    const SKILL_MULTIPLIER_TOTAL: f64 = 1.12;
    const COMBINED_SNAP_NORM_EXPONENT: f64 = 1.2;
    const SNAP_FLOW_K: f64 = 7.27;

    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        diff_objects: &'a [OsuDifficultyObject<'a>],
        with_slider_travel_dist: bool,
        magnetised_strength: Option<f64>,
        touch_device: bool,
        relax: bool,
    ) -> f64 {
        let snap_difficulty =
            SnapAimEvaluator::evaluate_diff_of(curr, diff_objects, with_slider_travel_dist)
                * Self::SKILL_MULTIPLIER_SNAP;
        let agility_difficulty =
            AgilityEvaluator::evaluate_diff_of(curr, diff_objects) * Self::SKILL_MULTIPLIER_AGILITY;
        let flow_difficulty =
            FlowAimEvaluator::evaluate_diff_of(curr, diff_objects, with_slider_travel_dist)
                * Self::SKILL_MULTIPLIER_FLOW;


        let mut total_difficulty =
            Self::calculate_total_value(snap_difficulty, agility_difficulty, flow_difficulty, touch_device, relax);

        // Magnetised mod
        if let Some(strength) = magnetised_strength {
            total_difficulty *= 1.0 - strength;
        }


        // OD bonus
        let result = total_difficulty * (0.985 + curr.overall_difficulty.max(0.0).powf(2.0) / 4000.0);
        result
    }

    fn calculate_total_value(
        snap_difficulty: f64,
        agility_difficulty: f64,
        flow_difficulty: f64,
        touch_device: bool,
        relax: bool,
    ) -> f64 {
        let mut snap_difficulty = snap_difficulty;
        let mut flow_difficulty = flow_difficulty;

        // We compare flow to combined snap and agility because snap by itself
        // doesn't have enough difficulty to be above flow on streams.
        // Agility measures the rate of cursor velocity changes while snapping,
        // so snapping every circle on a stream requires enormous agility at
        // which point it's easier to flow.
        let mut combined_snap_difficulty = norm(
            Self::COMBINED_SNAP_NORM_EXPONENT,
            [snap_difficulty, agility_difficulty],
        );

        let p_snap = Self::calculate_snap_flow_probability(
            flow_difficulty / combined_snap_difficulty,
        );
        let p_flow = 1.0 - p_snap;

        // TouchDevice mod
        if touch_device {
            // we don't adjust agility here since agility represents TD difficulty in a decent enough way
            snap_difficulty = snap_difficulty.powf(0.89);
            combined_snap_difficulty = norm(
                Self::COMBINED_SNAP_NORM_EXPONENT,
                [snap_difficulty, agility_difficulty],
            );
        }

        // // Relax mod
        if relax {
            // combined_snap_difficulty *= 0.8;
            flow_difficulty *= 0.7;
        }

        let total_difficulty =
            combined_snap_difficulty * p_snap + flow_difficulty * p_flow;

        total_difficulty * Self::SKILL_MULTIPLIER_TOTAL
    }

    /// A logistic function that turns the ratio of snap:flow into the probability
    /// of snapping/flowing.
    ///
    /// Constraints satisfied:
    /// - P(snap) + P(flow) = 1
    /// - P(snap) = f(snap/flow), P(flow) = f(flow/snap) (symmetric)
    /// - 0 <= f(x) <= 1
    fn calculate_snap_flow_probability(ratio: f64) -> f64 {
        if ratio == 0.0 {
            return 0.0;
        }

        if ratio.is_nan() {
            return 1.0;
        }

        let funny_value = -Self::SNAP_FLOW_K * ratio.ln();

        logistic(funny_value, 0.0, -1.0, None)
    }
}
