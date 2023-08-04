use crate::{config, config::RxEvent, config::TxEvent, factory_registry::FactoryEntries};
use log::debug;
use parking_lot::Mutex;
use rocket::{
    form::{Context, Contextual, Form},
    fs::{relative, FileServer},
    http::Status,
    post,
    serde::json::Json,
    tokio, State,
};
use rocket::{get, routes};
use rocket_dyn_templates::{context, Template};
use std::sync::Arc;
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub(crate) mod forms;

#[get("/config")]
async fn configuration(event_state: &State<EventStateHolder>) -> Template {
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
    bus: &State<BusHolder>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            debug!("Config Submission: {:#?}", submission);

            // Broadcast the new configuration to the driver
            let new_config = submission.try_into().expect("Bad conversion");

            bus.0
                .send(RxEvent::UpdateMatrixConfig(new_config))
                .await
                .expect("Could not send value");

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

#[get("/factories")]
fn factories(factories: &State<FactoryEntries>) -> Json<&FactoryEntries> {
    Json(factories.inner())
}

struct BusHolder(tokio::sync::mpsc::Sender<RxEvent>);

/// Holds the state of items that can be set via the event bus
struct EventState {
    current_config: Option<config::HardwareConfig>,
}

struct EventStateHolder(Arc<Mutex<EventState>>);

/// A HTTP server instance that serves the REST API as well as other debugging pages
pub struct HttpServer {
    cancel_token: CancellationToken,
    http_instance: Option<JoinHandle<()>>,
    event_receiver_task: Option<JoinHandle<()>>,
}

impl HttpServer {
    /// Creates a new instance of the HTTP server
    ///
    /// # Arguments
    /// * `event_sender` - The sender end of a channel that will be used to send commands back to the Render Factory
    /// * `event_receiver` - The receiver end of a channel that will be used receive commands from the Render Factory
    /// * `factories` - FactoryEntries from a constructed `FactoryRegistry`
    pub fn new(
        event_sender: tokio::sync::mpsc::Sender<RxEvent>,
        mut event_receiver: tokio::sync::mpsc::Receiver<TxEvent>,
        factories: FactoryEntries,
    ) -> Self {
        let event_holder = Arc::new(Mutex::new(EventState {
            current_config: None,
        }));

        let cancel_token = CancellationToken::new();

        // This task will receive events and then update the `EventState`
        let event_task_holder = event_holder.clone();
        let event_receiver_task = tokio::task::spawn(async move {
            while let Some(event) = event_receiver.recv().await {
                match event {
                    TxEvent::UpdateMatrixConfig(config) => {
                        event_task_holder.lock().current_config = Some(config);
                    }
                }
            }
        });

        let http_cancel_token = cancel_token.clone();
        let http_instance = tokio::task::spawn(async move {
            let http_instance = rocket::build()
                // .mount(
                //     "/",
                //     routes![
                //         configuration,
                //         submit_configuration,
                //         transit_config,
                //         submit_transit_config
                //     ],
                // )
                .mount("/api", routes![factories])
                // .mount("/", FileServer::from(relative!("/static")))
                // .attach(Template::fairing())
                .manage(BusHolder(event_sender))
                .manage(EventStateHolder(event_holder))
                .manage(factories)
                .launch();

            select! {
                _ = http_instance => {},
                _ = http_cancel_token.cancelled() => {}
            }
        });

        Self {
            cancel_token: cancel_token,
            http_instance: Some(http_instance),
            event_receiver_task: Some(event_receiver_task),
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.http_instance.take() {
            task_handle.abort();
        }

        if let Some(task_handle) = self.event_receiver_task.take() {
            task_handle.abort();
        }
    }
}
