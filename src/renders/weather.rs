use anyhow::Result;
use embedded_graphics::{
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, WebColors},
    text::Text,
    Drawable,
};
use embedded_layout::{
    layout::linear::{spacing, LinearLayout},
    prelude::Chain,
    view_group::Views,
};
use log::error;
use parking_lot::Mutex;
use rustic_pixel_display::render::Render;
use serde::Deserialize;
use std::{convert::Infallible, net::IpAddr, sync::Arc, time::Duration};
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use weer_api::{chrono::Utc, BaseApi, Client};

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
    location_name: String,
    temperature: f32,
    temperature_str: String,
    feels_like: f32,
    feels_like_str: String,
    wind: String,
    humidity: String,
}

impl From<weer_api::Forecast> for DisplayForecast {
    fn from(value: weer_api::Forecast) -> Self {
        Self {
            location_name: value.location.name.clone(),
            temperature: value.current.temp_f,
            temperature_str: format!("{} °F", value.current.temp_f),
            feels_like: value.current.feelslike_f,
            feels_like_str: format!("{} °F", value.current.feelslike_f),
            wind: format!("{} mph", value.current.wind_mph),
            humidity: format!("{} %", value.current.humidity),
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

        let color_from_temp = |temp: f32| -> Rgb888 {
            if temp > 50.0 && temp <= 70.0 {
                Rgb888::GREEN
            } else if temp > 70.0 && temp <= 80.0 {
                Rgb888::YELLOW
            } else if temp > 80.0 && temp <= 90.0 {
                Rgb888::RED
            } else if temp > 90.0 && temp <= 100.0 {
                Rgb888::CSS_PURPLE
            } else if temp > 100.0 {
                Rgb888::CSS_MAGENTA
            } else if temp > 40.0 && temp <= 50.0 {
                Rgb888::YELLOW
            } else if temp > 30.0 && temp <= 40.0 {
                Rgb888::RED
            } else if temp > 20.0 && temp <= 30.0 {
                Rgb888::CSS_PURPLE
            } else if temp <= 20.0 {
                Rgb888::CSS_MAGENTA
            } else {
                Rgb888::WHITE
            }
        };

        LinearLayout::vertical(
            Chain::new(Text::new(
                &display_state.location_name,
                Point::zero(),
                MonoTextStyle::new(&mono_font::iso_8859_1::FONT_7X13, Rgb888::WHITE),
            ))
            .append(
                LinearLayout::horizontal(Views::new(&mut [
                    Text::new(
                        "Temperature: ",
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::iso_8859_1::FONT_6X9, Rgb888::WHITE),
                    ),
                    Text::new(
                        &display_state.temperature_str,
                        Point::zero(),
                        MonoTextStyle::new(
                            &mono_font::iso_8859_1::FONT_6X9,
                            color_from_temp(display_state.temperature),
                        ),
                    ),
                ]))
                .arrange(),
            )
            .append(
                LinearLayout::horizontal(Views::new(&mut [
                    Text::new(
                        "Feels like: ",
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::iso_8859_1::FONT_6X9, Rgb888::WHITE),
                    ),
                    Text::new(
                        &display_state.feels_like_str,
                        Point::zero(),
                        MonoTextStyle::new(
                            &mono_font::iso_8859_1::FONT_6X9,
                            color_from_temp(display_state.feels_like),
                        ),
                    ),
                ]))
                .arrange(),
            )
            .append(
                LinearLayout::horizontal(Views::new(&mut [
                    Text::new(
                        "Wind: ",
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::iso_8859_1::FONT_6X9, Rgb888::WHITE),
                    ),
                    Text::new(
                        &display_state.wind,
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::iso_8859_1::FONT_6X9, Rgb888::WHITE),
                    ),
                ]))
                .arrange(),
            )
            .append(
                LinearLayout::horizontal(Views::new(&mut [
                    Text::new(
                        "Humidity: ",
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::iso_8859_1::FONT_6X9, Rgb888::WHITE),
                    ),
                    Text::new(
                        &display_state.humidity,
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::iso_8859_1::FONT_6X9, Rgb888::WHITE),
                    ),
                ]))
                .arrange(),
            ),
        )
        .with_spacing(spacing::FixedMargin(2))
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
