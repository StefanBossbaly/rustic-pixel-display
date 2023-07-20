use crate::config::{self};
use embedded_graphics::mono_font;
use rocket::{FromForm, FromFormField};
use serde::Serialize;
use std::str::FromStr;

#[derive(Debug, FromForm, Serialize)]
#[allow(dead_code)]
pub(crate) struct HardwareConfigForm<'a> {
    #[field(validate = one_of(["AdafruitHat", "AdafruitHatPwm", "Regular", "RegularPi1", "Classic", "ClassicPi1"]), default="Regular")]
    pub(crate) hardware_mapping: &'a str,

    #[field(validate = range(1..), default = 64)]
    pub(crate) rows: usize,

    #[field(validate = range(1..), default = 64)]
    pub(crate) cols: usize,

    #[field(validate = range(1..), default = 120)]
    pub(crate) refresh_rate: usize,

    #[field(validate = one_of(["Automatic", "BCM2708", "BCM2835", "BCM2709", "BCM2836", "BCM2837", "BCM2711"]), default="Automatic")]
    pub(crate) pi_chip: &'a str,

    #[field(validate = range(1..=11), default = 11)]
    pub(crate) pwm_bits: usize,

    #[field(validate = range(100..=300), default = 130)]
    pub(crate) pwm_lsb_nanoseconds: u32,

    #[field(validate = range(0..), default = 0)]
    pub(crate) slowdown: u32,

    #[field(validate = one_of(["True", "False"]), default="False")]
    pub(crate) interlaced: &'a str,

    #[field(validate = range(0..), default = 0)]
    pub(crate) dither_bits: usize,

    #[field(validate = range(1..), default = 1)]
    pub(crate) chain_length: usize,

    #[field(validate = range(1..), default = 1)]
    pub(crate) parallel: usize,

    #[field(validate = one_of(["None", "FM6126", "FM6127"]), default="None")]
    pub(crate) panel_type: &'a str,

    #[field(validate = one_of(["None", "Stripe", "Checkered", "Spiral", "ZStripe08", "ZStripe44", "ZStripe80", "Coreman", "Kaler2Scan",
    "P10Z", "QiangLiQ8", "InversedZStripe", "P10Outdoor1R1G1B1", "P10Outdoor1R1G1B2", "P10Outdoor1R1G1B3", "P10Coreman",
    "P8Outdoor1R1G1B", "FlippedStripe", "P10Outdoor32x16HalfScan"]), default="None")]
    pub(crate) multiplexing: &'a str,

    #[field(validate = one_of(["direct", "shiftregister", "directabcdline", "abcshiftregister", "sm5266"]), default="direct")]
    pub(crate) row_setter: &'a str,

    #[field(validate = one_of(["rgb", "rbg", "grb", "gbr", "brg", "bgr"]), default="rgb")]
    pub(crate) led_sequence: &'a str,
}

impl<'a> TryFrom<&'a config::HardwareConfig> for HardwareConfigForm<'a> {
    type Error = Box<dyn std::error::Error>;

    fn try_from(config: &'a config::HardwareConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            hardware_mapping: config.hardware_mapping.as_ref(),
            rows: config.rows,
            cols: config.cols,
            refresh_rate: config.refresh_rate,
            pi_chip: match &config.pi_chip {
                Some(pi_chip) => pi_chip.as_ref(),
                None => "Automatic",
            },
            pwm_bits: config.pwm_bits,
            pwm_lsb_nanoseconds: config.pwm_lsb_nanoseconds,
            slowdown: config.slowdown.unwrap_or(0),
            interlaced: if config.interlaced { "True" } else { "False" },
            dither_bits: config.dither_bits,
            chain_length: config.chain_length,
            parallel: config.parallel,
            panel_type: match &config.panel_type {
                Some(panel_type) => panel_type.as_ref(),
                None => "None",
            },
            multiplexing: match &config.multiplexing {
                Some(multiplexing) => multiplexing.as_ref(),
                None => "None",
            },
            row_setter: config.row_setter.as_ref(),
            led_sequence: config.led_sequence.as_ref(),
        })
    }
}

impl<'a> TryFrom<&HardwareConfigForm<'a>> for config::HardwareConfig {
    type Error = Box<dyn std::error::Error>;

    fn try_from(form: &HardwareConfigForm<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            hardware_mapping: config::HardwareMapping::from_str(form.hardware_mapping)?,
            rows: form.rows,
            cols: form.cols,
            refresh_rate: form.refresh_rate,
            pi_chip: match form.pi_chip {
                "Automatic" => None,
                _ => Some(config::PiChip::from_str(form.pi_chip)?),
            },
            pwm_bits: form.pwm_bits,
            pwm_lsb_nanoseconds: form.pwm_lsb_nanoseconds,
            slowdown: Some(form.slowdown),
            interlaced: form.interlaced == "True",
            dither_bits: form.dither_bits,
            chain_length: form.chain_length,
            parallel: form.parallel,
            panel_type: match form.panel_type {
                "None" => None,
                _ => Some(config::PanelType::from_str(form.panel_type)?),
            },
            multiplexing: match form.multiplexing {
                "None" => None,
                _ => Some(config::MultiplexMapperType::from_str(form.multiplexing)?),
            },
            row_setter: config::RowAddressSetterType::from_str(form.row_setter)?,
            led_sequence: config::LedSequence::from_str(form.led_sequence)?,
        })
    }
}

#[derive(Debug, PartialEq, FromFormField, Clone, Copy)]
pub(crate) enum Font {
    #[field(value = "4x6")]
    FourBySix,
    #[field(value = "5x7")]
    FiveBySeven,
    #[field(value = "5x8")]
    FiveByEight,
    #[field(value = "6x9")]
    SixByNine,
    #[field(value = "6x10")]
    SixByTen,
    #[field(value = "6x12")]
    SixByTwelve,
    #[field(value = "6x13")]
    SixByThirteen,
    #[field(value = "6x13 Bold")]
    SixByThirteenBold,
    #[field(value = "6x13 Italic")]
    SixByThirteenItalic,
    #[field(value = "7x13")]
    SevenByThirteen,
    #[field(value = "7x13 Bold")]
    SevenByThirteenBold,
    #[field(value = "7x13 Italic")]
    SevenByThirteenItalic,
    #[field(value = "7x14")]
    SevenByFourteen,
    #[field(value = "7x14 Bold")]
    SevenByFourteenBold,
    #[field(value = "8x13")]
    EightByThirteen,
    #[field(value = "8x13 Bold")]
    EightByThirteenBold,
    #[field(value = "8x13 Italic")]
    EightByThirteenItalic,
    #[field(value = "9x15")]
    NineByFifteen,
    #[field(value = "9x15 Bold")]
    NineByFifteenBold,
    #[field(value = "9x18")]
    NineByEighteen,
    #[field(value = "9x18 Bold")]
    NineByEighteenBold,
    #[field(value = "10x20")]
    TenByTwenty,
}

impl From<Font> for mono_font::MonoFont<'static> {
    fn from(value: Font) -> Self {
        match value {
            Font::FourBySix => mono_font::ascii::FONT_4X6,
            Font::FiveBySeven => mono_font::ascii::FONT_5X7,
            Font::FiveByEight => mono_font::ascii::FONT_5X8,
            Font::SixByNine => mono_font::ascii::FONT_6X9,
            Font::SixByTen => mono_font::ascii::FONT_6X10,
            Font::SixByTwelve => mono_font::ascii::FONT_6X12,
            Font::SixByThirteen => mono_font::ascii::FONT_6X13,
            Font::SixByThirteenBold => mono_font::ascii::FONT_6X13_BOLD,
            Font::SixByThirteenItalic => mono_font::ascii::FONT_6X13_ITALIC,
            Font::SevenByThirteen => mono_font::ascii::FONT_7X13,
            Font::SevenByThirteenBold => mono_font::ascii::FONT_7X13_BOLD,
            Font::SevenByThirteenItalic => mono_font::ascii::FONT_7X13_ITALIC,
            Font::SevenByFourteen => mono_font::ascii::FONT_7X14,
            Font::SevenByFourteenBold => mono_font::ascii::FONT_7X14_BOLD,
            Font::EightByThirteen => mono_font::ascii::FONT_8X13,
            Font::EightByThirteenBold => mono_font::ascii::FONT_8X13_BOLD,
            Font::EightByThirteenItalic => mono_font::ascii::FONT_8X13_ITALIC,
            Font::NineByFifteen => mono_font::ascii::FONT_9X15,
            Font::NineByFifteenBold => mono_font::ascii::FONT_9X15_BOLD,
            Font::NineByEighteen => mono_font::ascii::FONT_9X18,
            Font::NineByEighteenBold => mono_font::ascii::FONT_9X18_BOLD,
            Font::TenByTwenty => mono_font::ascii::FONT_10X20,
        }
    }
}

#[derive(Debug, FromForm, Serialize)]
#[allow(dead_code)]
pub(crate) struct TransitConfigForm<'a> {
    #[field()]
    pub(crate) home_assistant_url: &'a str,

    #[field()]
    pub(crate) home_assistant_bearer_token: &'a str,

    #[field()]
    pub(crate) person_entity_id: &'a str,
}

// impl<'a> From<&TransitConfigForm<'a>> for TransitTrackerConfig {
//     fn from(form: &TransitConfigForm<'a>) -> Self {
//         Self {
//             home_assistant_url: form.home_assistant_url.to_string(),
//             home_assistant_bearer_token: form.home_assistant_bearer_token.to_string(),
//             person_entity_id: form.person_entity_id.to_string(),
//         }
//     }
// }
