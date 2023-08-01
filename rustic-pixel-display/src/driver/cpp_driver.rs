use super::HardwareDriver;
use crate::config::{
    HardwareConfig, HardwareMapping, LedSequence, MultiplexMapperType, PanelType,
    RowAddressSetterType,
};
use anyhow::{anyhow, Result};
use rpi_led_matrix::{LedCanvas, LedMatrix, LedMatrixOptions, LedRuntimeOptions};

pub struct CppHardwareDriver {
    matrix: LedMatrix,
}

pub struct CombinedConfig {
    pub matrix_options: LedMatrixOptions,
    pub runtime_options: LedRuntimeOptions,
}

impl HardwareDriver for CppHardwareDriver {
    type Config = CombinedConfig;
    type Canvas = LedCanvas;

    fn new(config: Self::Config) -> anyhow::Result<Self> {
        let matrix =
            LedMatrix::new(Some(config.matrix_options), Some(config.runtime_options)).unwrap();

        Ok(Self { matrix })
    }

    fn create_canvas(&mut self) -> Box<Self::Canvas> {
        Box::new(self.matrix.canvas())
    }

    fn display_canvas(&mut self, canvas: Box<Self::Canvas>) -> Box<Self::Canvas> {
        Box::new(self.matrix.swap(*canvas))
    }
}

impl TryFrom<HardwareConfig> for CombinedConfig {
    type Error = Box<dyn std::error::Error>;

    fn try_from(value: HardwareConfig) -> Result<Self, Self::Error> {
        let mut matrix_options = LedMatrixOptions::default();
        let mut runtime_options = LedRuntimeOptions::default();

        matrix_options.set_hardware_mapping(match value.hardware_mapping {
            HardwareMapping::AdafruitHat => "adafruit-hat",
            HardwareMapping::AdafruitHatPwm => "adafruit-hat-pwm",
            HardwareMapping::Regular => "regular",
            HardwareMapping::RegularPi1 => "regular", // TODO: Have to verify
            HardwareMapping::Classic => "regular",    // TODO: Have to verify
            HardwareMapping::ClassicPi1 => "regular", // TODO: Have to verify
        });
        matrix_options.set_rows(value.rows as u32);
        matrix_options.set_cols(value.cols as u32);
        matrix_options.set_chain_length(value.chain_length as u32);
        matrix_options.set_parallel(value.parallel as u32);
        matrix_options.set_pwm_bits(value.pwm_bits as u8)?;
        matrix_options.set_pwm_lsb_nanoseconds(value.pwm_lsb_nanoseconds);
        matrix_options.set_brightness(100)?; // TODO: Have to include in HardwareConfig
        matrix_options.set_scan_mode(match value.interlaced {
            true => 1,
            false => 0,
        });
        matrix_options.set_led_rgb_sequence(match value.led_sequence {
            LedSequence::Rgb => "RGB",
            LedSequence::Rbg => "RBG",
            LedSequence::Grb => "GRB",
            LedSequence::Gbr => "GBR",
            LedSequence::Brg => "BRG",
            LedSequence::Bgr => "BGR",
        });
        matrix_options.set_pixel_mapper_config("");
        matrix_options.set_hardware_pulsing(false);
        matrix_options.set_refresh_rate(false);
        matrix_options.set_inverse_colors(false);
        matrix_options.set_multiplexing(if let Some(multiplexing) = value.multiplexing {
            match multiplexing {
                MultiplexMapperType::Stripe => Ok(1),
                MultiplexMapperType::Checkered => Ok(2),
                MultiplexMapperType::Spiral => Ok(3),
                MultiplexMapperType::ZStripe08 => Ok(4),
                MultiplexMapperType::ZStripe44 => Ok(5),
                MultiplexMapperType::ZStripe80 => Err(anyhow!(
                    "MultiplexMapperType::ZStripe80 not supported by C++ driver"
                )),
                MultiplexMapperType::Coreman => Ok(6),
                MultiplexMapperType::Kaler2Scan => Ok(7),
                MultiplexMapperType::P10Z => Ok(9),
                MultiplexMapperType::QiangLiQ8 => Ok(10),
                MultiplexMapperType::InversedZStripe => Ok(11),
                MultiplexMapperType::P10Outdoor1R1G1B1 => Ok(12),
                MultiplexMapperType::P10Outdoor1R1G1B2 => Ok(13),
                MultiplexMapperType::P10Outdoor1R1G1B3 => Ok(14),
                MultiplexMapperType::P10Coreman => Ok(15),
                MultiplexMapperType::P8Outdoor1R1G1B => Ok(16),
                MultiplexMapperType::FlippedStripe => Err(anyhow!(
                    "MultiplexMapperType::FlippedStripe not supported by C++ driver"
                )),
                MultiplexMapperType::P10Outdoor32x16HalfScan => Err(anyhow!(
                    "MultiplexMapperType::P10Outdoor32x16HalfScan not supported by C++ driver"
                )),
            }
        } else {
            Ok(0)
        }?);
        matrix_options.set_row_addr_type(match value.row_setter {
            RowAddressSetterType::Direct => 0,
            RowAddressSetterType::ShiftRegister => 1,
            RowAddressSetterType::DirectABCDLine => 2,
            RowAddressSetterType::ABCShiftRegister => 4,
            RowAddressSetterType::SM5266 => panic!("Not Supported!"),
        });
        matrix_options.set_limit_refresh(value.refresh_rate as u32);
        matrix_options.set_pwm_dither_bits(value.dither_bits as u32);
        matrix_options.set_panel_type(if let Some(panel_type) = value.panel_type {
            match panel_type {
                PanelType::FM6126 => "FM6126A",
                PanelType::FM6127 => "FM6127",
            }
        } else {
            ""
        });

        if let Some(slowdown) = value.slowdown {
            runtime_options.set_gpio_slowdown(slowdown);
        }

        Ok(Self {
            matrix_options,
            runtime_options,
        })
    }
}
