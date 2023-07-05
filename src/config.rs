use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[derive(Debug, Clone)]
pub enum RxEvent {
    UpdateMatrixConfig(HardwareConfig),
}

#[derive(Debug, Clone)]
pub enum TxEvent {
    UpdateMatrixConfig(HardwareConfig),
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "PascalCase", ascii_case_insensitive)]
pub enum HardwareMapping {
    AdafruitHat,
    AdafruitHatPwm,
    Regular,
    RegularPi1,
    Classic,
    ClassicPi1,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub enum PiChip {
    BCM2708,
    BCM2709,
    BCM2711,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub enum PanelType {
    FM6126,
    FM6127,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "PascalCase", ascii_case_insensitive)]
pub enum MultiplexMapperType {
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
pub enum RowAddressSetterType {
    Direct,
    ShiftRegister,
    DirectABCDLine,
    ABCShiftRegister,
    SM5266,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub enum LedSequence {
    Rgb,
    Rbg,
    Grb,
    Gbr,
    Brg,
    Bgr,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct HardwareConfig {
    pub hardware_mapping: HardwareMapping,
    pub rows: usize,
    pub cols: usize,
    pub refresh_rate: usize,
    pub pi_chip: Option<PiChip>,
    pub pwm_bits: usize,
    pub pwm_lsb_nanoseconds: u32,
    pub slowdown: Option<u32>,
    pub interlaced: bool,
    pub dither_bits: usize,
    pub chain_length: usize,
    pub parallel: usize,
    pub panel_type: Option<PanelType>,
    pub multiplexing: Option<MultiplexMapperType>,
    pub row_setter: RowAddressSetterType,
    pub led_sequence: LedSequence,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TransitConfig {
    pub home_assistant_url: String,
    pub home_assistant_bearer_token: String,
    pub person_entity_id: String,
}
