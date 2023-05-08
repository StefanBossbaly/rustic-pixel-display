use rocket::form::{Context, Contextual, Form, FromForm};
use rocket::http::Status;
use rocket::{Build, Rocket, State};
use rocket_dyn_templates::Template;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::config;
use crate::led_driver::LedDriver;

#[derive(Debug, FromForm)]
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

#[get("/config")]
fn configuration() -> Template {
    Template::render("config", &Context::default())
}

struct DriverState(Arc<Mutex<LedDriver>>);

#[post("/config", data = "<form>")]
fn submit_configuration<'r>(
    form: Form<Contextual<'r, HardwareConfigForm<'r>>>,
    driver_state: &State<DriverState>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);

            let new_config = submission.try_into().expect("Bad conversion");
            let mut lock = driver_state.0.lock().expect("lock shared data");
            lock.update_config(new_config);

            Template::render("config", &form.context)
        }
        None => Template::render("config", &form.context),
    };

    (form.context.status(), template)
}

pub(crate) fn build_rocket(led_driver: Arc<Mutex<LedDriver>>) -> Rocket<Build> {
    rocket::build()
        .mount("/", routes![configuration, submit_configuration])
        .attach(Template::fairing())
        .manage(DriverState(led_driver))
}
