use super::HardwareDriver;
use crate::config::HardwareConfig;
use anyhow::{Context, Result};
use rpi_led_panel::{Canvas, RGBMatrix, RGBMatrixConfig};
use std::str::FromStr;

pub struct RustHardwareDriver {
    matrix: RGBMatrix,
    offscreen_canvas: Option<Box<Canvas>>,
}

impl HardwareDriver for RustHardwareDriver {
    type Config = RGBMatrixConfig;
    type Canvas = Canvas;

    fn new(config: Self::Config) -> Result<Self> {
        let result = RGBMatrix::new(config, 0).context("Invalid configuration provided")?;

        Ok(Self {
            matrix: result.0,
            offscreen_canvas: Some(result.1),
        })
    }

    fn create_canvas(&mut self) -> Box<Self::Canvas> {
        self.offscreen_canvas.take().unwrap()
    }

    fn display_canvas(&mut self, canvas: Box<Self::Canvas>) -> Box<Self::Canvas> {
        self.matrix.update_on_vsync(canvas)
    }
}

impl TryFrom<HardwareConfig> for RGBMatrixConfig {
    type Error = Box<dyn std::error::Error>;

    fn try_from(config: HardwareConfig) -> Result<Self, Self::Error> {
        Ok(RGBMatrixConfig {
            hardware_mapping: rpi_led_panel::HardwareMapping::from_str(
                config.hardware_mapping.as_ref(),
            )?,
            rows: config.rows,
            cols: config.cols,
            refresh_rate: config.refresh_rate,
            pi_chip: match config.pi_chip {
                Some(pi_chip) => Some(rpi_led_panel::PiChip::from_str(pi_chip.as_ref())?),
                None => None,
            },
            pwm_bits: config.pwm_bits,
            pwm_lsb_nanoseconds: config.pwm_lsb_nanoseconds,
            slowdown: config.slowdown,
            interlaced: config.interlaced,
            dither_bits: config.dither_bits,
            chain_length: config.chain_length,
            parallel: config.parallel,
            panel_type: match config.panel_type {
                Some(panel_type) => Some(rpi_led_panel::PanelType::from_str(panel_type.as_ref())?),
                None => None,
            },
            multiplexing: match config.multiplexing {
                Some(multiplexing) => Some(rpi_led_panel::MultiplexMapperType::from_str(
                    multiplexing.as_ref(),
                )?),
                None => None,
            },
            row_setter: rpi_led_panel::RowAddressSetterType::from_str(config.row_setter.as_ref())?,
            led_sequence: rpi_led_panel::LedSequence::from_str(config.led_sequence.as_ref())?,
        })
    }
}
