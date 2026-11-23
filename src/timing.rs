use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct FrameTiming {
    pub simulation_steps: u32,
    pub simulation_step: Duration,
    pub interpolation_alpha: f64,
    pub dropped_time: Duration,
}

pub struct FrameClock {
    simulation_step: Duration,
    max_steps_per_frame: u32,
    budget: Duration,
    remainder: Duration,
}

impl FrameClock {
    /// Returns `None` if either input is zero or their product exceeds `Duration`.
    pub fn new(simulation_step: Duration, max_steps_per_frame: u32) -> Option<Self> {
        let budget = simulation_step.checked_mul(max_steps_per_frame)?;
        if budget.is_zero() {
            return None;
        }
        Some(Self {
            simulation_step,
            max_steps_per_frame,
            budget,
            remainder: Duration::ZERO,
        })
    }

    pub fn advance(&mut self, elapsed: Duration) -> FrameTiming {
        let accepted = elapsed.min(self.budget - self.remainder);
        let accumulated = self.remainder + accepted;
        let simulation_steps = (accumulated.as_nanos() / self.simulation_step.as_nanos()) as u32;
        debug_assert!(simulation_steps <= self.max_steps_per_frame);
        self.remainder = accumulated - self.simulation_step * simulation_steps;

        FrameTiming {
            simulation_steps,
            simulation_step: self.simulation_step,
            interpolation_alpha: self
                .remainder
                .div_duration_f64(self.simulation_step)
                .min(1.0_f64.next_down()),
            dropped_time: elapsed - accepted,
        }
    }
}
