use crate::{
    GameMods,
    any::difficulty::skills::strain_decay,
    osu::difficulty::{
        evaluators,
        object::OsuDifficultyObject,
    },
    util::difficulty::logistic,
};

pub struct Reading {
    has_hidden_mod: bool,
    has_touch_device_mod: bool,
    has_magnetised_mod: bool,
    magnetised_strength: f64,
    has_relax_mod: bool,
    has_autopilot_mod: bool,
    overall_difficulty: f64,

    current_strain: f64,
    reduced_note_count: f64,
    reduced_duration: Option<f64>,
    object_difficulties: Vec<f64>,
    object_weight_sum: f64,
}

impl Reading {
    const SKILL_MULTIPLIER: f64 = 2.5;
    const STRAIN_DECAY_BASE: f64 = 0.8;
    const REDUCED_DIFFICULTY_DURATION: f64 = 60_000.0;

    pub fn new(mods: &GameMods, overall_difficulty: f64) -> Self {
        Self {
            has_hidden_mod: mods.hd(),
            has_touch_device_mod: mods.td(),
            has_magnetised_mod: mods.attraction_strength().is_some(),
            magnetised_strength: mods.attraction_strength().unwrap_or(0.0),
            has_relax_mod: mods.rx(),
            has_autopilot_mod: mods.ap(),
            overall_difficulty,
            current_strain: 0.0,
            reduced_note_count: 0.0,
            reduced_duration: None,
            object_difficulties: Vec::with_capacity(256),
            object_weight_sum: 0.0,
        }
    }

    pub fn process<'a>(
        &mut self,
        curr: &OsuDifficultyObject<'a>,
        diff_objects: &[OsuDifficultyObject<'a>],
    ) {
        let strain = self.strain_value_at(curr, diff_objects);
        self.object_difficulties.push(strain);
    }

    fn strain_value_at<'a>(
        &mut self,
        curr: &OsuDifficultyObject<'a>,
        diff_objects: &[OsuDifficultyObject<'a>],
    ) -> f64 {
        let decay = strain_decay(curr.delta_time, Self::STRAIN_DECAY_BASE);

        self.current_strain *= decay;
        self.current_strain +=
            self.calculate_adjusted_difficulty(curr, diff_objects) * (1.0 - decay) * Self::SKILL_MULTIPLIER;

        if self.reduced_duration.is_none() {
            self.reduced_duration =
                Some(curr.start_time + Self::REDUCED_DIFFICULTY_DURATION);
        }

        if let Some(reduced_duration) = self.reduced_duration {
            if curr.start_time <= reduced_duration {
                self.reduced_note_count += 1.0;
            }
        }

        self.current_strain
    }

    fn calculate_adjusted_difficulty<'a>(
        &self,
        curr: &OsuDifficultyObject<'a>,
        diff_objects: &[OsuDifficultyObject<'a>],
    ) -> f64 {
        let mut difficulty =
            evaluators::reading::evaluate_diff_of(curr, diff_objects, self.has_hidden_mod);

        if self.has_touch_device_mod {
            difficulty = difficulty.powf(0.89);
        }

        if self.has_magnetised_mod {
            difficulty *= 1.0 - self.magnetised_strength;
        }

        if self.has_relax_mod {
            difficulty *= 0.4;
        }

        if self.has_autopilot_mod {
            difficulty *= 0.1;
        }

        difficulty *= 0.825
            + self.overall_difficulty.powf(2.2) / 1125.0;

        difficulty
    }

    pub fn difficulty_value(&mut self) -> f64 {
        if self.object_difficulties.is_empty() {
            return 0.0;
        }

        let difficulties = self.get_transformed_difficulties();

        let mut sorted: Vec<f64> = difficulties.into_iter().filter(|&v| v > 0.0).collect();
        sorted.sort_by(|a, b| b.total_cmp(a));

        const HARMONIC_SCALE: f64 = 1.0;
        const DECAY_EXPONENT: f64 = 0.9;

        let mut difficulty = 0.0;
        let mut index = 0;

        self.object_weight_sum = 0.0;

        for obj in sorted {
            let weight = (1.0 + HARMONIC_SCALE / (1.0 + index as f64))
                / ((index as f64).powf(DECAY_EXPONENT)
                    + 1.0
                    + HARMONIC_SCALE / (1.0 + index as f64));

            self.object_weight_sum += weight;
            difficulty += obj * weight;
            index += 1;
        }

        difficulty
    }

    fn get_transformed_difficulties(&self) -> Vec<f64> {
        let mut difficulties: Vec<f64> = self
            .object_difficulties
            .iter()
            .copied()
            .filter(|&v| v > 0.0)
            .collect();

        const REDUCED_DIFFICULTY_BASE_LINE: f64 = 0.0;

        let count = difficulties.len().min(self.reduced_note_count as usize);

        for i in 0..count {
            let scale = f64::log10(
                1.0 + 9.0 * (i as f64 / self.reduced_note_count).clamp(0.0, 1.0),
            );
            difficulties[i] *=
                REDUCED_DIFFICULTY_BASE_LINE + (1.0 - REDUCED_DIFFICULTY_BASE_LINE) * scale;
        }

        difficulties
    }

    pub fn count_top_weighted_object_difficulties(&self, difficulty_value: f64) -> f64 {
        if self.object_difficulties.is_empty() {
            return 0.0;
        }

        if self.object_weight_sum == 0.0 {
            return 0.0;
        }

        let consistent_top_note = difficulty_value / self.object_weight_sum;

        if consistent_top_note == 0.0 {
            return 0.0;
        }

        self.object_difficulties
            .iter()
            .map(|d| logistic(d / consistent_top_note, 1.15, 5.0, Some(1.1)))
            .sum()
    }

    pub fn difficulty_to_performance(difficulty: f64) -> f64 {
        4.0 * difficulty.powf(3.0)
    }
}
