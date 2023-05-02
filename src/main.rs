#[macro_use]
extern crate rocket;

use rocket::form::{Context, Contextual, Form, FromForm};
use rocket::http::Status;

use rocket_dyn_templates::Template;

use tokio::join;
use tokio::sync::broadcast;

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

// NOTE: We use `Contextual` here because we want to collect all submitted form
// fields to re-render forms with submitted values on error. If you have no such
// need, do not use `Contextual`. Use the equivalent of `Form<Submit<'_>>`.
#[post("/config", data = "<form>")]
fn submit_config<'r>(form: Form<Contextual<'r, HardwareConfig<'r>>>) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);
            Template::render("config", &form.context)
        }
        None => Template::render("config", &form.context),
    };

    (form.context.status(), template)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_tx, mut rx) = broadcast::channel::<i8>(10);

    let rocket = rocket::build()
        .mount("/", routes![config, submit_config])
        .attach(Template::fairing())
        .ignite()
        .await?
        .launch();

    let background_task = tokio::spawn(async move {
        tokio::select! {
            _ = rx.recv() => {
                println!("Received a message!");
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                println!("Timed out!");
            }
        }
    });

    let (rocket_result, background_result) = join!(rocket, background_task);
    rocket_result?;
    background_result?;

    Ok(())
}
