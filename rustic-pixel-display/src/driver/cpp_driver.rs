use super::HardwareDriver;
use crate::config::HardwareConfig;
use anyhow::Result;
use rpi_led_matrix::{LedCanvas, LedMatrix, LedMatrixOptions};

pub struct CppHardwareDriver {
    matrix: LedMatrix,
}

impl HardwareDriver for CppHardwareDriver {
    type Config = LedMatrixOptions;
    type Canvas = LedCanvas;

    fn new(config: Self::Config) -> anyhow::Result<Self> {
        let matrix = LedMatrix::new(Some(config), None).unwrap();

        Ok(Self { matrix })
    }

    fn create_canvas(&mut self) -> Box<Self::Canvas> {
        Box::new(self.matrix.canvas())
    }

    fn display_canvas(&mut self, canvas: Box<Self::Canvas>) -> Box<Self::Canvas> {
        Box::new(self.matrix.swap(*canvas))
    }
}

impl TryFrom<HardwareConfig> for LedMatrixOptions {
    type Error = Box<dyn std::error::Error>;

    fn try_from(value: HardwareConfig) -> Result<Self, Self::Error> {
        let mut config = LedMatrixOptions::new();

        config.set_rows(value.rows as u32);
        config.set_cols(value.cols as u32);
        config.set_limit_refresh(value.refresh_rate as u32);
        config.set_pwm_bits(value.pwm_bits as u8)?;
        config.set_pwm_lsb_nanoseconds(value.pwm_lsb_nanoseconds);

        if let Some(_slowdown) = value.slowdown {
            // TODO
        }

        config.set_chain_length(value.chain_length as u32);
        config.set_parallel(value.parallel as u32);

        if let Some(_panel_type) = value.panel_type {
            //config.set_panel_type(panel_type);
        }

        Ok(config)
    }
}
