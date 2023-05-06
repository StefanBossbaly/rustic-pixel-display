#[macro_use]
extern crate rocket;

use std::str::FromStr;
use std::sync::Mutex;

use led_driver::LedDriver;
use rocket::form::{Context, Contextual, Form, FromForm};
use rocket::http::Status;
use rocket::State;

use rocket_dyn_templates::Template;

use rpi_led_panel::{HardwareMapping, LedSequence, RGBMatrixConfig, RowAddressSetterType};

mod led_driver;

#[derive(Debug, FromForm)]
#[allow(dead_code)]
struct HardwareConfig<'a> {
    #[field(validate = one_of(["AdafruitHat", "AdafruitHatPwm", "Regular", "RegularPi1", "Classic", "ClassicPi1"]), default="Regular")]
    hardware_mapping: &'a str,

    #[field(validate = range(1..), default = 64)]
    rows: usize,

    #[field(validate = range(1..), default = 64)]
    cols: usize,

    #[field(validate = range(1..), default = 120)]
    refresh_rate: usize,

    #[field(validate = range(1..), default = 1)]
    chain_length: usize,

    #[field(validate = range(1..), default = 1)]
    parallel: usize,
}

#[get("/config")]
fn config() -> Template {
    Template::render("config", &Context::default())
}

struct DriverState(Mutex<LedDriver>);

// NOTE: We use `Contextual` here because we want to collect all submitted form
// fields to re-render forms with submitted values on error. If you have no such
// need, do not use `Contextual`. Use the equivalent of `Form<Submit<'_>>`.
#[post("/config", data = "<form>")]
fn submit_config<'r>(
    form: Form<Contextual<'r, HardwareConfig<'r>>>,
    driver_state: &State<DriverState>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);

            let config = RGBMatrixConfig {
                hardware_mapping: HardwareMapping::from_str(submission.hardware_mapping)
                    .expect("Invalid hardware mapping"),
                rows: submission.rows,
                cols: submission.cols,
                refresh_rate: submission.refresh_rate,
                pi_chip: None,
                pwm_bits: 6,
                pwm_lsb_nanoseconds: 130,
                slowdown: None,
                interlaced: false,
                dither_bits: 0,
                chain_length: submission.chain_length,
                parallel: submission.parallel,
                panel_type: None,
                multiplexing: None,
                row_setter: RowAddressSetterType::Direct,
                led_sequence: LedSequence::Rgb,
            };

            {
                let mut lock = driver_state.0.lock().expect("lock shared data");
                lock.update_config(config);
            }

            Template::render("config", &form.context)
        }
        None => Template::render("config", &form.context),
    };

    (form.context.status(), template)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut led_driver = LedDriver::new();
    led_driver.start(RGBMatrixConfig {
        hardware_mapping: HardwareMapping::regular(),
        rows: 64,
        cols: 64,
        refresh_rate: 120,
        pi_chip: None,
        pwm_bits: 6,
        pwm_lsb_nanoseconds: 130,
        slowdown: None,
        interlaced: false,
        dither_bits: 0,
        chain_length: 1,
        parallel: 1,
        panel_type: None,
        multiplexing: None,
        row_setter: RowAddressSetterType::Direct,
        led_sequence: LedSequence::Rgb,
    });

    let _rocket = rocket::build()
        .mount("/", routes![config, submit_config])
        .attach(Template::fairing())
        .manage(DriverState(Mutex::new(led_driver)))
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
