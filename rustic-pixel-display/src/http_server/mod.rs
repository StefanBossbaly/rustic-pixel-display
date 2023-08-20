use std::{convert::Infallible, io::Read, net::ToSocketAddrs, sync::Arc};

use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use parking_lot::Mutex;
use rouille::{input::json::JsonError, router, try_or_400, Request, Response, Server};
use serde_json::json;
use tokio::runtime::Handle;

use crate::{
    factory_registry::{FactoryEntries, FactoryRegistry},
    render::RenderFactory,
};

fn json_input_to_reader(request: &Request) -> Result<impl Read + '_, JsonError> {
    if let Some(header) = request.header("Content-Type") {
        if !header.starts_with("application/json") {
            return Err(JsonError::WrongContentType);
        }
    } else {
        return Err(JsonError::WrongContentType);
    }

    if let Some(body) = request.data() {
        Ok(body)
    } else {
        Err(JsonError::BodyAlreadyExtracted)
    }
}

pub fn build_api_server<A, D, F>(
    addr: A,
    runtime: Handle,
    factory_registry: Arc<Mutex<FactoryRegistry<F, D>>>,
) -> Server<impl Send + Sync + 'static + Fn(&Request) -> Response>
where
    A: ToSocketAddrs,
    D: DrawTarget<Color = Rgb888, Error = Infallible> + 'static,
    F: RenderFactory<D> + 'static,
{
    Server::new(addr, move |request| {
        let mut factory_registry_unlock = factory_registry.lock();

        // This request will be processed in rouille's executor. Because of this, we need to ensure that
        // any async task that are launched are tied to our tokio runtime. The enter() ensures that if a task
        // is spawned, it will be spawned on this runtime.
        let _guard = runtime.enter();

        router!(request,
            (GET) (/) => {
                // For the sake of the example we just put a dummy route for `/` so that you see
                // something if you connect to the server with a browser.
                Response::text("Hello! Unfortunately there is nothing to see here.")
            },
            (GET) (/factory/discovery) => {
                let entries: FactoryEntries = (&*factory_registry_unlock).into();
                Response::json(&entries)
            },
            (POST) (/factory/load/{render_name: String}) => {
                // Attempt to read the JSON input from the request body
                let json_reader = try_or_400!(json_input_to_reader(request));

                // Attempt to load the render into the registry
                try_or_400!(factory_registry_unlock.load(&render_name, json_reader));

                Response::text("Render loaded successfully")
            },
            (POST) (/factory/unload/{render_name: String}) => {
                try_or_400!(factory_registry_unlock.unload(&render_name));
                Response::text("Render unloaded successfully")
            },
            (POST) (/factory/select/{render_name: String}) => {
                try_or_400!(factory_registry_unlock.select(&render_name));
                Response::text("Render selected successfully")
            },
            (POST) (/factory/clear) => {
                Response::json(&match factory_registry_unlock.clear() {
                    Some(_) => json!({"success" : true}),
                    None => json!({"success" : false})
                })
            },
            // If none of the other blocks matches the request, return a 404 response.
            _ => Response::empty_404()
        )
    })
    .unwrap()
}
