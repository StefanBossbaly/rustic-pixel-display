use std::{convert::Infallible, io::Read, net::ToSocketAddrs, sync::Arc};

use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use parking_lot::Mutex;
use rouille::{input::json::JsonError, router, try_or_400, try_or_404, Request, Response, Server};
use serde::Serialize;
use tokio::runtime::Handle;
use try_or_400::ErrJson;
use uuid::Uuid;

use crate::{
    registry::{Registry, RegistryError},
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

#[derive(Serialize)]
struct FactoryEntry<'a> {
    name: &'a str,
    description: &'a str,
}

#[derive(Serialize)]
struct RenderEntry<'a> {
    id: String,
    factory_name: &'a str,
}

#[derive(Serialize)]
struct LoadResponse {
    id: String,
}

#[derive(Serialize)]
enum LayoutValues {
    Single,
    SplitWidth,
    SplitHeight,
    Split4,
}

#[derive(Serialize)]
struct LayoutConfig<'a> {
    layout: LayoutValues,
    renders: Vec<RenderEntry<'a>>,
}

pub struct HttpInstance<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    factory_registry: Registry<F, D>,
}

pub fn build_api_server<A, D, F>(
    addr: A,
    runtime: Handle,
    factory_registry: Arc<Mutex<Registry<F, D>>>,
) -> Server<impl Send + Sync + 'static + Fn(&Request) -> Response>
where
    A: ToSocketAddrs,
    D: DrawTarget<Color = Rgb888, Error = Infallible> + 'static,
    F: RenderFactory<D> + 'static,
{
    Server::new(addr, move |request| {
        let mut registry_unlock = factory_registry.lock();

        // This request will be processed in rouille's executor. Because of this, we need to ensure that
        // any async task that are launched are tied to our tokio runtime. The enter() ensures that if a task
        // is spawned, it will be spawned on this runtime.
        let _guard = runtime.enter();

        router!(request,
            (GET) (/render/active) => {
                Response::json(
                    &registry_unlock
                        .render_iter()
                        .map(|(uuid, render)| RenderEntry {
                            id: uuid.to_string(),
                            factory_name: &render.factory_name,
                        })
                        .collect::<Vec<_>>(),
                )
            },
            (DELETE) (/render/{uuid: Uuid}) => {
                try_or_404!(registry_unlock.unload(uuid));
                Response::empty_204()
            },
            (GET) (/factory/discovery) => {
                Response::json(
                    &registry_unlock
                        .factory_iter()
                        .map(|(_, factory)| FactoryEntry {
                            name: factory.render_name(),
                            description: factory.render_description(),
                        })
                        .collect::<Vec<_>>(),
                )
            },
            (GET) (/factory/details/{_factory_name: String}) => {
                // TODO: Implement
                Response::empty_400()
            },
            (POST) (/factory/load/{render_name: String}) => {
                // Attempt to read the JSON input from the request body
                let json_reader = try_or_400!(json_input_to_reader(request));

                // Attempt to load the render into the registry
                let uuid = match registry_unlock.load(&render_name, json_reader) {
                    Ok(uuid) => uuid,
                    Err(e) => match e {
                        RegistryError::FactoryNotFound(_) => return Response::empty_404(),
                        _ => {
                            let json_error = ErrJson::from_err(&e);
                            return Response::json(&json_error).with_status_code(400);
                        }
                    }
                };

                Response::json(&LoadResponse {
                    id: uuid.to_string()
                })
            },
            (POST) (/layout_manager/select/{uuid: Uuid}) => {
                try_or_404!(registry_unlock.select(uuid));
                Response::empty_204()
            },
            // If none of the other blocks matches the request, return a 404 response.
            _ => Response::empty_404()
        )
    })
    .unwrap()
}
