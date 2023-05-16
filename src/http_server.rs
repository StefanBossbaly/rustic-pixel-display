use anyhow::Result;
use embedded_graphics::mono_font;
use rocket::{
    form::{Context, Contextual, Form, FromForm},
    fs::{relative, FileServer},
    http::Status,
    Build, Rocket, State,
};
use rocket_dyn_templates::{context, Template};
use serde::Serialize;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use crate::config;
use crate::led_driver::LedDriver;
use crate::render::{DebugTextConfig, DebugTextRender};

#[derive(Debug, FromForm, Serialize)]
#[allow(dead_code)]
struct HardwareConfigForm<'a> {
    #[field(validate = one_of(["AdafruitHat", "AdafruitHatPwm", "Regular", "RegularPi1", "Classic", "ClassicPi1"]), default="Regular")]
    hardware_mapping: &'a str,

    #[field(validate = range(1..), default = 64)]
    rows: usize,

    #[field(validate = range(1..), default = 64)]
    cols: usize,

    #[field(validate = range(1..), default = 120)]
    refresh_rate: usize,

    #[field(validate = one_of(["Automatic", "BCM2708", "BCM2835", "BCM2709", "BCM2836", "BCM2837", "BCM2711"]), default="Automatic")]
    pi_chip: &'a str,

    #[field(validate = range(1..=11), default = 11)]
    pwm_bits: usize,

    #[field(validate = range(100..=300), default = 130)]
    pwm_lsb_nanoseconds: u32,

    #[field(validate = range(0..), default = 0)]
    slowdown: u32,

    #[field(validate = one_of(["True", "False"]), default="False")]
    interlaced: &'a str,

    #[field(validate = range(0..), default = 0)]
    dither_bits: usize,

    #[field(validate = range(1..), default = 1)]
    chain_length: usize,

    #[field(validate = range(1..), default = 1)]
    parallel: usize,

    #[field(validate = one_of(["None", "FM6126", "FM6127"]), default="None")]
    panel_type: &'a str,

    #[field(validate = one_of(["None", "Stripe", "Checkered", "Spiral", "ZStripe08", "ZStripe44", "ZStripe80", "Coreman", "Kaler2Scan",
    "P10Z", "QiangLiQ8", "InversedZStripe", "P10Outdoor1R1G1B1", "P10Outdoor1R1G1B2", "P10Outdoor1R1G1B3", "P10Coreman",
    "P8Outdoor1R1G1B", "FlippedStripe", "P10Outdoor32x16HalfScan"]), default="None")]
    multiplexing: &'a str,

    #[field(validate = one_of(["direct", "shiftregister", "directabcdline", "abcshiftregister", "sm5266"]), default="direct")]
    row_setter: &'a str,

    #[field(validate = one_of(["rgb", "rbg", "grb", "gbr", "brg", "bgr"]), default="rgb")]
    led_sequence: &'a str,
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

#[derive(Debug, FromForm)]
struct DebugTextForm<'a> {
    text: &'a str,
    x: i32,
    y: i32,
    font: Font,
}

impl<'a> TryFrom<&DebugTextForm<'a>> for DebugTextConfig {
    type Error = Box<dyn std::error::Error>;

    fn try_from(form: &DebugTextForm<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            text: form.text.to_string(),
            x: form.x,
            y: form.y,
            font: form.font,
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

#[get("/config")]
fn configuration(driver_state: &State<DriverState>) -> Template {
    let unlocked_state = driver_state.0.lock().expect("Poisoned mutex");
    let config = unlocked_state.get_config().expect("Could not get config");

    match config {
        None => Template::render("config", Context::default()),
        Some(config_value) => {
            let form = HardwareConfigForm {
                hardware_mapping: config_value.hardware_mapping.as_ref(),
                rows: config_value.rows,
                cols: config_value.cols,
                refresh_rate: config_value.refresh_rate,
                pi_chip: match &config_value.pi_chip {
                    Some(pi_chip) => pi_chip.as_ref(),
                    None => "Automatic",
                },
                pwm_bits: config_value.pwm_bits,
                pwm_lsb_nanoseconds: config_value.pwm_lsb_nanoseconds,
                slowdown: config_value.slowdown.unwrap_or(0),
                interlaced: match config_value.interlaced {
                    true => "True",
                    false => "False",
                },
                dither_bits: config_value.dither_bits,
                chain_length: config_value.chain_length,
                parallel: config_value.parallel,
                panel_type: match &config_value.panel_type {
                    Some(panel_type) => panel_type.as_ref(),
                    None => "None",
                },
                multiplexing: match &config_value.multiplexing {
                    Some(multiplexing) => multiplexing.as_ref(),
                    None => "None",
                },
                row_setter: config_value.row_setter.as_ref(),
                led_sequence: config_value.led_sequence.as_ref(),
            };

            let ctx = context! {
                initial_values: form,
            };

            dbg!(&ctx);

            Template::render("config", ctx)
        }
    }
}

struct DriverState(Arc<Mutex<LedDriver>>);
struct DebugTextRenderState(Arc<Mutex<Box<DebugTextRender>>>);

#[post("/config", data = "<form>")]
fn submit_configuration<'r>(
    form: Form<Contextual<'r, HardwareConfigForm<'r>>>,
    driver_state: &State<DriverState>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);

            let new_config = submission.try_into().expect("Bad conversion");
            let mut lock = driver_state.0.lock().expect("Poisoned mutex");
            lock.update_config(new_config)
                .expect("Could not update config");

            Template::render("config", &form.context)
        }
        None => Template::render("config", &form.context),
    };

    (form.context.status(), template)
}

#[get("/debug_text")]
fn debug_text() -> Template {
    Template::render("debug_text", &Context::default())
}

#[post("/debug_text", data = "<form>")]
fn submit_debug_text<'a>(
    form: Form<Contextual<'a, DebugTextForm<'a>>>,
    debug_text_render_state: &State<DebugTextRenderState>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);

            let mut render_unlock = debug_text_render_state.0.lock().expect("Poisoned mutex");
            let config: DebugTextConfig = submission.try_into().expect("Bad conversion");
            render_unlock.update_config(config);

            Template::render("debug_text", &form.context)
        }
        None => Template::render("debug_text", &form.context),
    };

    (form.context.status(), template)
}

pub(crate) fn build_rocket(
    led_driver: Arc<Mutex<LedDriver>>,
    render: Arc<Mutex<Box<DebugTextRender>>>,
) -> Rocket<Build> {
    rocket::build()
        .mount(
            "/",
            routes![
                configuration,
                submit_configuration,
                debug_text,
                submit_debug_text
            ],
        )
        .mount("/", FileServer::from(relative!("/static")))
        .attach(Template::fairing())
        .manage(DriverState(led_driver))
        .manage(DebugTextRenderState(render))
}
