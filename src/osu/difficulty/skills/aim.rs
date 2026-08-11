use crate::{
    any::difficulty::{
        object::IDifficultyObject,
        skills::strain_decay,
    },
    osu::difficulty::{
        evaluators::AimEvaluator,
        object::OsuDifficultyObject,
        skills::strain::{StrainPeak, count_top_weighted_strains_variable, variable_length_difficulty_value},
    },
    util::float_ext::FloatExt,
};

pub struct Aim {
    pub include_sliders: bool,
    has_autopilot_mod: bool,
    magnetised_strength: Option<f64>,
    has_touch_device_mod: bool,
    has_relax_mod: bool,
    current_strain: f64,
    slider_strains: Vec<f64>,
    object_difficulties: Vec<f64>,

    // VariableLengthStrainSkill fields
    current_section_peak: f64,
    current_section_begin: f64,
    current_section_end: f64,
    strain_peaks: Vec<StrainPeak>,
    total_length: f64,
    max_stored_length: f64,
    queued_strains: Vec<(f64, f64)>, // (strain_value, start_time)
}

impl Aim {
    const STRAIN_DECAY_BASE: f64 = 0.2;
    const DECAY_WEIGHT: f64 = 0.9;
    const MAX_SECTION_LENGTH: f64 = 400.0;

    pub fn new(
        include_sliders: bool,
        has_autopilot_mod: bool,
        magnetised_strength: Option<f64>,
        has_touch_device_mod: bool,
        has_relax_mod: bool,
    ) -> Self {
        let max_stored_length = 11.0 / (1.0 - Self::DECAY_WEIGHT);

        Self {
            include_sliders,
            has_autopilot_mod,
            magnetised_strength,
            has_touch_device_mod,
            has_relax_mod,
            current_strain: 0.0,
            slider_strains: Vec::with_capacity(64),
            object_difficulties: Vec::with_capacity(256),
            current_section_peak: 0.0,
            current_section_begin: 0.0,
            current_section_end: 0.0,
            strain_peaks: Vec::with_capacity(256),
            total_length: 0.0,
            max_stored_length,
            queued_strains: Vec::new(),
        }
    }

    pub fn process<'a>(
        &mut self,
        curr: &OsuDifficultyObject<'a>,
        diff_objects: &[OsuDifficultyObject<'a>],
    ) {
        // If we're on the first object, set up the first section
        if curr.idx == 0 {
            self.current_section_begin = curr.start_time;
            self.current_section_end = self.current_section_begin + Self::MAX_SECTION_LENGTH;

            let strain = self.strain_value_at(curr, diff_objects);
            self.current_section_peak = strain;
            self.object_difficulties.push(strain);
            return;
        }

        self.backfill_peaks(curr, diff_objects);

        let current_strain = self.strain_value_at(curr, diff_objects);
        self.object_difficulties.push(current_strain);

        // If the current strain is larger than the current peak, begin a new peak
        if current_strain > self.current_section_peak {
            self.queued_strains.clear();
            self.save_current_peak(curr.start_time - self.current_section_begin);

            self.current_section_begin = curr.start_time;
            self.current_section_end = self.current_section_begin + Self::MAX_SECTION_LENGTH;
            self.current_section_peak = current_strain;
        } else {
            // Empty the queue of smaller elements
            while let Some(&(last_strain, _)) = self.queued_strains.last() {
                if last_strain < current_strain {
                    self.queued_strains.pop();
                } else {
                    break;
                }
            }

            self.queued_strains.push((current_strain, curr.start_time));
        }
    }

    fn backfill_peaks<'a>(
        &mut self,
        curr: &OsuDifficultyObject<'a>,
        diff_objects: &[OsuDifficultyObject<'a>],
    ) {
        while curr.start_time > self.current_section_end {
            self.save_current_peak(self.current_section_end - self.current_section_begin);
            self.current_section_begin = self.current_section_end;

            if let Some((strain, start_time)) = self.queued_strains.first().copied() {
                self.queued_strains.remove(0);

                self.current_section_end = start_time + Self::MAX_SECTION_LENGTH;
                self.current_section_peak =
                    self.calculate_initial_strain(self.current_section_begin, curr, diff_objects);
                self.current_section_peak = self.current_section_peak.max(strain);
            } else {
                self.current_section_end = self.current_section_begin + Self::MAX_SECTION_LENGTH;
                self.current_section_peak =
                    self.calculate_initial_strain(self.current_section_begin, curr, diff_objects);
            }
        }
    }

    fn save_current_peak(&mut self, section_length: f64) {
        self.strain_peaks
            .push(StrainPeak::new(self.current_section_peak, section_length));
        self.total_length += section_length;

        // Remove peaks that are too deep to contribute
        while self.total_length > self.max_stored_length * Self::MAX_SECTION_LENGTH {
            if let Some(last) = self.strain_peaks.pop() {
                self.total_length -= last.section_length;
            } else {
                break;
            }
        }
    }

    fn calculate_initial_strain(
        &self,
        time: f64,
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let prev_start_time = curr
            .previous(0, diff_objects)
            .map_or(0.0, |obj| obj.start_time);

        self.current_strain * strain_decay(time - prev_start_time, Self::STRAIN_DECAY_BASE)
    }

    fn strain_value_at(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        if self.has_autopilot_mod {
            return 0.0;
        }

        let decay = strain_decay(curr.adjusted_delta_time, Self::STRAIN_DECAY_BASE);

        self.current_strain *= decay;
        self.current_strain += AimEvaluator::evaluate_diff_of(
            curr,
            diff_objects,
            self.include_sliders,
            self.magnetised_strength,
            self.has_touch_device_mod,
            self.has_relax_mod,
        ) * (1.0 - decay);

        if curr.base.is_slider() {
            self.slider_strains.push(self.current_strain);
        }

        self.current_strain
    }

    /// Returns the current strain peaks for difficulty calculation.
    pub fn get_current_strain_peaks(&self) -> Vec<StrainPeak> {
        let final_peak = StrainPeak::new(
            self.current_section_peak,
            self.current_section_end - self.current_section_begin,
        );

        let mut peaks = self.strain_peaks.clone();
        peaks.push(final_peak);
        peaks
    }

    pub fn difficulty_value(&self) -> f64 {
        let peaks = self.get_current_strain_peaks_for_calc();
        variable_length_difficulty_value(&peaks, Self::DECAY_WEIGHT, Self::MAX_SECTION_LENGTH)
    }

    fn get_current_strain_peaks_for_calc(&self) -> Vec<StrainPeak> {
        let final_peak = StrainPeak::new(
            self.current_section_peak,
            self.current_section_end - self.current_section_begin,
        );

        let mut peaks = self.strain_peaks.clone();
        peaks.push(final_peak);
        peaks
    }

    pub fn cloned_difficulty_value(&self) -> f64 {
        self.difficulty_value()
    }

    pub fn count_top_weighted_strains(&self, difficulty_value: f64) -> f64 {
        count_top_weighted_strains_variable(
            &self.object_difficulties,
            difficulty_value,
            Self::DECAY_WEIGHT,
        )
    }

    pub fn get_difficult_sliders(&self) -> f64 {
        if self.slider_strains.is_empty() {
            return 0.0;
        }

        let max_slider_strain = self
            .slider_strains
            .iter()
            .copied()
            .fold(0.0, f64::max);

        if FloatExt::eq(max_slider_strain, 0.0) {
            return 0.0;
        }

        self.slider_strains
            .iter()
            .copied()
            .map(|strain| 1.0 / (1.0 + f64::exp(-(strain / max_slider_strain * 12.0 - 6.0))))
            .sum()
    }

    pub fn slider_strains(&self) -> &[f64] {
        &self.slider_strains
    }

    pub fn difficulty_to_performance(difficulty: f64) -> f64 {
        super::strain::difficulty_to_performance(difficulty)
    }
}
