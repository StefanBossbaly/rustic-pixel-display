use crate::{config, config::RxEvent, config::TxEvent};
use log::debug;
use parking_lot::Mutex;
use rocket::{
    form::{Context, Contextual, Form},
    fs::{relative, FileServer},
    http::Status,
    post, Build, Rocket, State,
};
use rocket::{get, routes};
use rocket_dyn_templates::{context, Template};
use std::sync::Arc;

pub(crate) mod forms;

struct EventStateHolder {
    driver_sender: std::sync::mpsc::Sender<RxEvent>,
    current_config: Option<config::HardwareConfig>,
}

struct EventState(Arc<Mutex<EventStateHolder>>);

#[get("/config")]
async fn configuration(event_state: &State<EventState>) -> Template {
    // Unlock the bus state and clone the current configuration. We could
    // avoid the clone by holding the lock while we convert but it could
    // possible cause contention issues.
    let config = { event_state.0.lock().current_config.clone() };

    match config {
        None => Template::render("config", Context::default()),
        Some(config_value) => {
            let form = forms::HardwareConfigForm {
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

            Template::render(
                "config",
                context! {
                    initial_values: form,
                },
            )
        }
    }
}

#[post("/config", data = "<form>")]
async fn submit_configuration<'r>(
    form: Form<Contextual<'r, forms::HardwareConfigForm<'r>>>,
    event_state: &State<EventState>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            debug!("Config Submission: {:#?}", submission);

            // Broadcast the new configuration to the driver
            let new_config = submission.try_into().expect("Bad conversion");

            {
                event_state
                    .0
                    .lock()
                    .driver_sender
                    .send(RxEvent::UpdateMatrixConfig(new_config))
                    .expect("Could not send value");
            } //drop(bus_state_unlocked)

            Template::render("config", &form.context)
        }
        None => Template::render("config", &form.context),
    };

    (form.context.status(), template)
}

#[get("/transit_config")]
fn transit_config() -> Template {
    Template::render("transit_config", Context::default())
}

#[post("/transit_config", data = "<form>")]
async fn submit_transit_config<'r>(
    form: Form<Contextual<'r, forms::TransitConfigForm<'r>>>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);

            Template::render("transit_config", &form.context)
        }
        None => Template::render("transit_config", &form.context),
    };

    (form.context.status(), template)
}

pub fn build_rocket(
    event_sender: std::sync::mpsc::Sender<RxEvent>,
    event_receiver: std::sync::mpsc::Receiver<TxEvent>,
) -> Rocket<Build> {
    let event_holder = Arc::new(Mutex::new(EventStateHolder {
        driver_sender: event_sender,
        current_config: None,
    }));

    let event_task_holder = event_holder.clone();

    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv() {
            match event {
                TxEvent::UpdateMatrixConfig(config) => {
                    event_task_holder.lock().current_config = Some(config);
                }
            }
        }
    });

    rocket::build()
        .mount(
            "/",
            routes![
                configuration,
                submit_configuration,
                transit_config,
                submit_transit_config
            ],
        )
        .mount("/", FileServer::from(relative!("/static")))
        .attach(Template::fairing())
        .manage(EventState(event_holder))
}
