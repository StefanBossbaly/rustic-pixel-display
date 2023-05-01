#[macro_use]
extern crate rocket;

use rocket::form::{Context, Contextual, Form, FromForm};
use rocket::http::Status;

use rocket_dyn_templates::Template;

use tokio::join;
use tokio::sync::broadcast;

#[derive(Debug, FromForm)]
struct HardwareConfig<'a> {
    hardware_mapping: &'a str,
    rows: usize,
    cols: usize,
    refresh_rate: usize,
    chain_length: usize,
    parallel: usize,
}

#[get("/")]
fn index() -> Template {
    Template::render("index", &Context::default())
}

// NOTE: We use `Contextual` here because we want to collect all submitted form
// fields to re-render forms with submitted values on error. If you have no such
// need, do not use `Contextual`. Use the equivalent of `Form<Submit<'_>>`.
#[post("/", data = "<form>")]
fn submit<'r>(form: Form<Contextual<'r, HardwareConfig<'r>>>) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);
            Template::render("index", &form.context)
        }
        None => Template::render("index", &form.context),
    };

    (form.context.status(), template)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) = broadcast::channel::<i8>(10);

    let rocket = rocket::build()
        .mount("/", routes![index, submit])
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
