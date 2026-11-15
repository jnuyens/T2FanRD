use std::io::{IsTerminal, Write};

use crate::{
    config::{FanConfig, SpeedCurve},
    error::{Error, Result},
    FanPath,
};

#[derive(Debug)]
pub struct FanController {
    manual_file: Option<std::fs::File>,
    output_file: std::fs::File,
    config: FanConfig,

    min_speed: u32,
    max_speed: u32,
}

impl FanController {
    pub fn new(path: &FanPath, config: FanConfig) -> Result<Self> {
        let min_speed = std::fs::read_to_string(path.min_speed_path())
            .map_err(Error::MinSpeedRead)?
            .trim()
            .parse()
            .map_err(Error::MinSpeedParse)?;

        let max_speed = std::fs::read_to_string(path.max_speed_path())
            .map_err(Error::MaxSpeedRead)?
            .trim_end()
            .parse()
            .map_err(Error::MaxSpeedParse)?;

        let mut open_options = std::fs::OpenOptions::new();
        open_options.write(true).truncate(true);

        let manual_file = path
            .manual_path()
            .map(|p| open_options.open(p))
            .transpose()
            .map_err(Error::FanOpen)?;

        let output_file = open_options
            .open(path.input_path())
            .map_err(Error::FanOpen)?;

        let this = Self {
            manual_file,
            output_file,
            config,
            min_speed,
            max_speed,
        };

        println!("Found fan: {this:#?}");
        Ok(this)
    }

    pub fn set_manual(&self, enabled: bool) -> Result<()> {
        self.manual_file
            .as_ref()
            .map_or(Ok(()), |mut p| {
                (&mut p).write_all(if enabled { b"1" } else { b"0" })
            })
            .map_err(Error::FanWrite)
    }

    pub fn set_speed(&self, mut speed: u32) -> Result<()> {
        if speed < self.min_speed {
            speed = self.min_speed;
        } else if speed > self.max_speed {
            speed = self.max_speed;
        }

        {
            let mut stdout = std::io::stdout().lock();
            if stdout.is_terminal() {
                print!("\x1b[1K\rSetting fan speed to {speed}");
                let _ = stdout.flush();
            }
        }

        write!(&self.output_file, "{speed}").map_err(Error::FanWrite)?;
        Ok(())
    }

    pub fn calc_speed(&self, temp: u8) -> u32 {
        if self.config.always_full_speed {
            return self.max_speed;
        }

        self.calc_speed_with(
            temp,
            self.config.low_temp,
            self.config.high_temp,
            self.config.speed_curve,
        )
    }

    /// Speed for `temp` on the given curve, scaled to this fan's range.
    pub fn calc_speed_with(
        &self,
        temp: u8,
        low_temp: u8,
        high_temp: u8,
        speed_curve: SpeedCurve,
    ) -> u32 {
        if temp <= low_temp {
            return self.min_speed;
        }
        if temp >= high_temp {
            return self.max_speed;
        }

        let temp = temp as u32;
        let low_temp = low_temp as u32;
        let high_temp = high_temp as u32;
        match speed_curve {
            SpeedCurve::Linear => {
                ((temp - low_temp) as f32 / (high_temp - low_temp) as f32
                    * (self.max_speed - self.min_speed) as f32) as u32
                    + self.min_speed
            }
            SpeedCurve::Exponential => {
                ((temp - low_temp).pow(3) as f32 / (high_temp - low_temp).pow(3) as f32
                    * (self.max_speed - self.min_speed) as f32) as u32
                    + self.min_speed
            }
            SpeedCurve::Logarithmic => {
                (((temp - low_temp) as f32).log((high_temp - low_temp) as f32)
                    * (self.max_speed - self.min_speed) as f32) as u32
                    + self.min_speed
            }
        }
    }
}
