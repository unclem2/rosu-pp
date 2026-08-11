use crate::util::{
    difficulty::logistic,
    float_ext::FloatExt,
    traits::{IEnumerable, IOrderedEnumerable},
};

#[expect(dead_code, reason = "used by other skills via OsuStrainSkill")]
pub trait OsuStrainSkill {
    const REDUCED_SECTION_COUNT: usize = 10;
    const REDUCED_STRAIN_BASELINE: f64 = 0.75;

    fn difficulty_to_performance(difficulty: f64) -> f64 {
        difficulty_to_performance(difficulty)
    }
}

/// A strain peak with its associated section length, used by VariableLengthStrainSkill.
#[derive(Clone, Copy, Debug)]
pub struct StrainPeak {
    pub value: f64,
    pub section_length: f64,
}

impl StrainPeak {
    pub fn new(value: f64, section_length: f64) -> Self {
        Self {
            value,
            section_length: section_length.round(),
        }
    }
}

/// Variable-length strain difficulty value with chunk-based reduction and continuous integration.
pub fn variable_length_difficulty_value(
    strain_peaks: &[StrainPeak],
    decay_weight: f64,
    max_section_length: f64,
) -> f64 {
    const REDUCED_SECTION_TIME: f64 = 4000.0;
    const REDUCED_STRAIN_BASELINE: f64 = 0.727;
    const CHUNK_SIZE: f64 = 20.0;

    // Filter out zero-value peaks
    let peaks: Vec<StrainPeak> = strain_peaks
        .iter()
        .copied()
        .filter(|p| p.value > 0.0)
        .collect();

    // Chunk-based reduction: split the highest strains into 20ms chunks
    let mut reduced_peaks: Vec<StrainPeak> = Vec::new();
    let mut skip_count = 0;
    let mut time = 0.0;

    while skip_count < peaks.len() && time < REDUCED_SECTION_TIME {
        let strain = peaks[skip_count];
        let mut added_time = 0.0;

        while added_time < strain.section_length {
            let scale = f64::log10(lerp(
                1.0,
                10.0,
                ((time + added_time) / REDUCED_SECTION_TIME).clamp(0.0, 1.0),
            ));

            reduced_peaks.push(StrainPeak::new(
                strain.value * lerp(REDUCED_STRAIN_BASELINE, 1.0, scale),
                CHUNK_SIZE.min(strain.section_length - added_time),
            ));

            added_time += CHUNK_SIZE;
        }

        time += strain.section_length;
        skip_count += 1;
    }

    // Add remaining peaks
    reduced_peaks.extend_from_slice(&peaks[skip_count..]);

    // Sort by value descending
    reduced_peaks.sort_by(|a, b| b.value.total_cmp(&a.value));

    // Continuous integration
    let mut difficulty = 0.0;
    let mut integrated_time = 0.0;

    for peak in &reduced_peaks {
        let start_time = integrated_time;
        let end_time = integrated_time + peak.section_length / max_section_length;

        let weight = decay_weight.powf(start_time) - decay_weight.powf(end_time);
        difficulty += peak.value * weight;

        integrated_time = end_time;
    }

    difficulty / (1.0 - decay_weight)
}

/// Count top weighted strains using per-object difficulties (for VariableLengthStrainSkill).
pub fn count_top_weighted_strains_variable(
    object_difficulties: &[f64],
    difficulty_value: f64,
    decay_weight: f64,
) -> f64 {
    if object_difficulties.is_empty() {
        return 0.0;
    }

    let consistent_top_strain = difficulty_value * (1.0 - decay_weight);

    if FloatExt::eq(consistent_top_strain, 0.0) {
        return object_difficulties.len() as f64;
    }

    object_difficulties
        .iter()
        .map(|s| logistic(*s / consistent_top_strain, 0.88, 10.0, Some(1.1)))
        .sum()
}

pub fn difficulty_value(
    current_strain_peaks: Vec<f64>,
    reduced_section_count: usize,
    reduced_strain_baseline: f64,
    decay_weight: f64,
) -> f64 {
    let mut difficulty = 0.0;
    let mut weight = 1.0;

    // * Sections with 0 strain are excluded to avoid worst-case time complexity of the following sort (e.g. /b/2351871).
    // * These sections will not contribute to the difficulty.
    let peaks = current_strain_peaks.cs_where(|&p| p > 0.0);

    let mut strains = peaks.cs_order_descending();

    for (i, strain) in strains.iter_mut().take(reduced_section_count).enumerate() {
        let clamped = f64::from((i as f32 / reduced_section_count as f32).clamp(0.0, 1.0));
        let scale = f64::log10(lerp(1.0, 10.0, clamped));
        *strain *= lerp(reduced_strain_baseline, 1.0, scale);
    }

    for strain in strains.cs_order_descending() {
        difficulty += strain * weight;
        weight *= decay_weight;
    }

    difficulty
}

pub fn count_top_weighted_sliders(slider_strains: &[f64], difficulty_value: f64) -> f64 {
    if slider_strains.is_empty() {
        return 0.0;
    }

    // * What would the top strain be if all strain values were identical
    let consistent_top_strain = difficulty_value / 10.0;

    if FloatExt::eq(consistent_top_strain, 0.0) {
        return 0.0;
    }

    slider_strains
        .iter()
        .map(|s| logistic(*s / consistent_top_strain, 0.88, 10.0, Some(1.1)))
        .sum()
}

pub fn difficulty_to_performance(difficulty: f64) -> f64 {
    f64::powf(5.0 * f64::max(1.0, difficulty / 0.0675) - 4.0, 3.0) / 100_000.0
}

const fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    start + (end - start) * amount
}
