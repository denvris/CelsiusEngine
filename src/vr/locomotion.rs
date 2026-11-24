use std::time::Duration;

const SMOOTH_SECONDS: f32 = 0.028;
const REVERSAL_METERS: f32 = 0.0025;
const BOB_METERS: std::ops::RangeInclusive<f32> = 0.011..=0.10;
const RISE_SECONDS: std::ops::RangeInclusive<f32> = 0.06..=0.42;
const STEP_SECONDS: std::ops::RangeInclusive<f32> = 0.20..=0.72;

#[derive(Clone, Copy)]
struct Sample {
    height: f32,
    time: f32,
}

/// Detects successive vertical bobs by their amplitude and cadence.
pub struct HeadBobDetector {
    filtered: Option<Sample>,
    extreme: Sample,
    trough: Option<Sample>,
    last_peak: Option<f32>,
    step_interval: Option<f32>,
}

impl HeadBobDetector {
    pub fn new(height: f32) -> Option<Self> {
        let sample = Sample { height, time: 0.0 };
        height.is_finite().then_some(Self {
            filtered: Some(sample),
            extreme: sample,
            trough: None,
            last_peak: None,
            step_interval: None,
        })
    }

    /// Pass `None` when the headset pose is lost. A fresh cadence is required after reacquisition.
    pub fn update(&mut self, height: Option<f32>, elapsed: Duration) -> bool {
        self.step_interval = None;
        let Some(height) = height.filter(|height| height.is_finite()) else {
            self.filtered = None;
            self.trough = None;
            self.last_peak = None;
            return false;
        };
        let dt = elapsed.as_secs_f32();
        let Some(previous) = self.filtered.filter(|_| dt <= 0.10) else {
            *self = Self::new(height).expect("finite tracking height");
            return false;
        };
        let sample = Sample {
            height: previous.height
                + (height - previous.height) * (1.0 - (-dt / SMOOTH_SECONDS).exp()),
            time: previous.time + dt,
        };
        self.filtered = Some(sample);

        if let Some(trough) = self.trough {
            if sample.height >= self.extreme.height {
                self.extreme = sample;
            } else if self.extreme.height - sample.height >= REVERSAL_METERS {
                let peak = self.extreme;
                self.extreme = sample;
                self.trough = None;
                if BOB_METERS.contains(&(peak.height - trough.height))
                    && RISE_SECONDS.contains(&(peak.time - trough.time))
                {
                    self.step_interval = self
                        .last_peak
                        .replace(peak.time)
                        .map(|last| peak.time - last)
                        .filter(|interval| STEP_SECONDS.contains(interval));
                    return self.step_interval.is_some();
                }
                self.last_peak = None;
            }
        } else if sample.height <= self.extreme.height {
            self.extreme = sample;
        } else if sample.height - self.extreme.height >= REVERSAL_METERS {
            self.trough = Some(self.extreme);
            self.extreme = sample;
        }
        false
    }

    pub fn step_interval_seconds(&self) -> Option<f32> {
        self.step_interval
    }
}
