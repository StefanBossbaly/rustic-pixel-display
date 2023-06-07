use rpi_led_panel::RGBMatrixConfig;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use strum_macros::{AsRefStr, EnumString};

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "PascalCase", ascii_case_insensitive)]
pub(crate) enum HardwareMapping {
    AdafruitHat,
    AdafruitHatPwm,
    Regular,
    RegularPi1,
    Classic,
    ClassicPi1,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub(crate) enum PiChip {
    BCM2708,
    BCM2709,
    BCM2711,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub(crate) enum PanelType {
    FM6126,
    FM6127,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "PascalCase", ascii_case_insensitive)]
pub(crate) enum MultiplexMapperType {
    Stripe,
    Checkered,
    Spiral,
    ZStripe08,
    ZStripe44,
    ZStripe80,
    Coreman,
    Kaler2Scan,
    P10Z,
    QiangLiQ8,
    InversedZStripe,
    P10Outdoor1R1G1B1,
    P10Outdoor1R1G1B2,
    P10Outdoor1R1G1B3,
    P10Coreman,
    P8Outdoor1R1G1B,
    FlippedStripe,
    P10Outdoor32x16HalfScan,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "PascalCase", ascii_case_insensitive)]
pub(crate) enum RowAddressSetterType {
    Direct,
    ShiftRegister,
    DirectABCDLine,
    ABCShiftRegister,
    SM5266,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub(crate) enum LedSequence {
    Rgb,
    Rbg,
    Grb,
    Gbr,
    Brg,
    Bgr,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub(crate) struct HardwareConfig {
    pub(crate) hardware_mapping: HardwareMapping,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) refresh_rate: usize,
    pub(crate) pi_chip: Option<PiChip>,
    pub(crate) pwm_bits: usize,
    pub(crate) pwm_lsb_nanoseconds: u32,
    pub(crate) slowdown: Option<u32>,
    pub(crate) interlaced: bool,
    pub(crate) dither_bits: usize,
    pub(crate) chain_length: usize,
    pub(crate) parallel: usize,
    pub(crate) panel_type: Option<PanelType>,
    pub(crate) multiplexing: Option<MultiplexMapperType>,
    pub(crate) row_setter: RowAddressSetterType,
    pub(crate) led_sequence: LedSequence,
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub(crate) struct TransitConfig {
    pub(crate) home_assistant_url: String,
    pub(crate) home_assistant_bearer_token: String,
    pub(crate) person_entity_id: String,
}
