use std::time::Duration;

const BOB_THRESHOLD_METERS: f32 = 0.025;
const NEUTRAL_BAND_METERS: f32 = 0.01;
const MIN_RISE_TIME: Duration = Duration::from_millis(90);
const MAX_RISE_TIME: Duration = Duration::from_millis(600);

enum Phase {
    Ready,
    Away { side: f32, age: Duration },
    WaitForNeutral,
}

/// Emits a step signal from tracking-space head height relative to a calibrated neutral pose.
pub struct HeadBobDetector {
    neutral_height: f32,
    phase: Phase,
}

impl HeadBobDetector {
    pub fn new(neutral_height: f32) -> Option<Self> {
        neutral_height.is_finite().then_some(Self {
            neutral_height,
            phase: Phase::WaitForNeutral,
        })
    }

    /// Pass `None` when head position is unavailable. A step signal leaves movement policy to the caller.
    pub fn update(&mut self, height_meters: Option<f32>, elapsed: Duration) -> bool {
        let Some(offset) = height_meters
            .map(|height| height - self.neutral_height)
            .filter(|offset| offset.is_finite())
        else {
            self.phase = Phase::WaitForNeutral;
            return false;
        };

        match self.phase {
            Phase::WaitForNeutral if offset.abs() <= NEUTRAL_BAND_METERS => {
                self.phase = Phase::Ready;
            }
            Phase::Ready if offset.abs() >= BOB_THRESHOLD_METERS => {
                self.phase = Phase::Away {
                    side: offset.signum(),
                    age: Duration::ZERO,
                };
            }
            Phase::Away { side, age } => {
                let age = age.saturating_add(elapsed);
                if age > MAX_RISE_TIME {
                    self.phase = Phase::WaitForNeutral;
                } else if offset * side <= NEUTRAL_BAND_METERS {
                    self.phase = if offset.abs() <= NEUTRAL_BAND_METERS {
                        Phase::Ready
                    } else {
                        Phase::WaitForNeutral
                    };
                    return age >= MIN_RISE_TIME;
                } else {
                    self.phase = Phase::Away { side, age };
                }
            }
            _ => {}
        }
        false
    }
}
