use anyhow::Result;
use embedded_graphics::{
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor},
    text::Text,
    Drawable,
};
use embedded_layout::{layout::linear::LinearLayout, prelude::Chain};
use log::error;
use parking_lot::Mutex;
use serde::Deserialize;
use std::{convert::Infallible, net::IpAddr, sync::Arc, time::Duration};
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use weer_api::{chrono::Utc, BaseApi, Client};

use crate::render::Render;

#[derive(Clone, Debug, Deserialize)]
pub enum Location {
    LatLon(f32, f32),
    City(String),
    Ip(Option<IpAddr>),
}

impl From<Location> for weer_api::Query {
    fn from(value: Location) -> Self {
        match value {
            Location::LatLon(lat, lon) => weer_api::Query::Coords(lat, lon),
            Location::City(city) => weer_api::Query::City(city),
            Location::Ip(ip) => weer_api::Query::Ip(ip),
        }
    }
}

#[derive(Debug, Default)]
struct DisplayForecast {
    temperature: String,
}

impl From<weer_api::Forecast> for DisplayForecast {
    fn from(value: weer_api::Forecast) -> Self {
        Self {
            temperature: format!("{} °F", value.current.temp_f),
        }
    }
}

pub struct Configuration {
    pub api_key: String,

    pub location: Location,
}

pub struct Weather {
    state: Arc<Mutex<DisplayForecast>>,

    /// Flag used to gracefully terminate the render and driver threads
    cancel_token: CancellationToken,

    /// Handle to the task used to update the SEPTA information
    update_forecast_handle: Option<JoinHandle<Result<()>>>,
}

impl Weather {
    pub fn new(config: Configuration) -> Self {
        let client = Client::new(&config.api_key, true);

        let display_state = Arc::new(Mutex::new(DisplayForecast::default()));
        let cancel_token = CancellationToken::new();

        let task_cancel_token = cancel_token.clone();
        let task_display_state = display_state.clone();

        let update_forecast_handle = tokio::task::spawn(async move {
            loop {
                let start_time = tokio::time::Instant::now();
                let refresh_duration;

                match client
                    .forecast()
                    .query(config.location.clone().into())
                    .dt(Utc::now())
                    .call()
                {
                    Ok(result) => {
                        *task_display_state.lock() = result.into();
                        refresh_duration = Duration::from_secs(30 * 60);
                    }
                    Err(e) => {
                        error!("Could not get updated information {e}");
                        refresh_duration = Duration::from_secs(30);
                    }
                }

                select! {
                    _ = tokio::time::sleep_until(start_time + refresh_duration) => {},
                    _ = task_cancel_token.cancelled() => break,
                }
            }

            Ok(())
        });

        Self {
            state: display_state,
            cancel_token,
            update_forecast_handle: Some(update_forecast_handle),
        }
    }
}

impl<D> Render<D> for Weather
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<()> {
        let display_state = self.state.lock();

        LinearLayout::vertical(Chain::new(Text::new(
            display_state.temperature.as_str(),
            Point::zero(),
            MonoTextStyle::new(&mono_font::iso_8859_1::FONT_10X20, Rgb888::WHITE),
        )))
        .arrange()
        .draw(canvas)
        .unwrap();

        Ok(())
    }
}

impl Drop for Weather {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.update_forecast_handle.take() {
            task_handle.abort();
        }
    }
}
