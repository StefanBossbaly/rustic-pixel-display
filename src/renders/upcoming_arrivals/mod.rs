use anyhow::Result;
use embedded_graphics::{
    image::Image,
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::{DrawTarget, PixelColor, Point, RgbColor},
    text::Text,
    Drawable,
};
use embedded_layout::{
    chain,
    layout::linear::{Horizontal, LinearLayout},
    prelude::{vertical, Chain},
    view_group::Views,
    View,
};
use embedded_layout::{layout::linear::spacing, prelude::Link};
use embedded_layout_macros::ViewGroup;
use log::error;
use parking_lot::Mutex;
use rustic_pixel_display::render::{Render, RenderFactory};
use septa_api::types::RegionalRailStop;
use serde::Deserialize;
use std::{convert::Infallible, io::Read, marker::PhantomData, sync::Arc, time::Duration};
use tinybmp::Bmp;
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use weer_api::chrono::{DateTime, FixedOffset};

use self::{amtrak_provider::AmtrakProvider, septa_provider::SeptaProvider};

mod amtrak_provider;
mod septa_provider;

#[derive(Debug, Clone, Copy)]
enum UpcomingTrainStatus {
    OnTime,
    Early(u32),
    Late(u32),
    Unknown,
}

#[derive(Debug, Clone)]
struct UpcomingTrain {
    /// The time the train is scheduled to arrive in the station
    schedule_arrival: DateTime<FixedOffset>,

    /// The final destination of the train
    destination_name: String,

    /// The unique identifier of the train
    train_id: String,

    /// The amount of time, in mins, that the train is late from its scheduled time. A negative value
    /// indicates the train is that many mins early.
    status: UpcomingTrainStatus,
}

#[derive(Debug, Default)]
struct UpcomingTrainsState {
    septa_arrivals: Vec<UpcomingTrain>,

    amtrak_arrivals: Vec<UpcomingTrain>,

    combined_arrivals: Vec<UpcomingTrain>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpcomingArrivalsConfig {
    pub septa_station: RegionalRailStop,
    pub amtrak_station: Option<String>,
    pub results: Option<u8>,
}

pub struct UpcomingArrivals {
    state: Arc<Mutex<UpcomingTrainsState>>,

    /// The regional rail stop
    station: RegionalRailStop,

    /// Flag used to gracefully terminate the render and driver threads
    cancel_token: CancellationToken,

    /// Handle to the task used to update the SEPTA information
    update_task_handle: Option<JoinHandle<Result<()>>>,
}

impl UpcomingArrivals {
    pub fn new(config: UpcomingArrivalsConfig) -> Self {
        let station = config.septa_station.clone();
        let state = Arc::new(Mutex::new(UpcomingTrainsState::default()));
        let cancel_token = CancellationToken::new();

        let task_cancel_token = cancel_token.clone();
        let task_state = state.clone();

        let update_task_handle: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            let septa_client = SeptaProvider::new(config.septa_station);

            let amtrak_client = config.amtrak_station.map(AmtrakProvider::new);

            loop {
                let refresh_time = tokio::time::Instant::now() + Duration::from_secs(60);

                let septa_arrivals = match septa_client.arrivals().await {
                    Ok(response) => Some(response),
                    Err(e) => {
                        error!("Could not get updated SEPTA arrivals {e}");
                        None
                    }
                };

                let amtrak_arrivals = if let Some(amtrak_client) = &amtrak_client {
                    match amtrak_client.arrivals().await {
                        Ok(response) => Some(response),
                        Err(e) => {
                            error!("Could not get updated Amtrak arrivals {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                {
                    let mut state_unlocked = task_state.lock();

                    if let Some(septa_arrivals) = septa_arrivals {
                        state_unlocked.septa_arrivals = septa_arrivals;
                    }

                    if let Some(amtrak_arrivals) = amtrak_arrivals {
                        state_unlocked.amtrak_arrivals = amtrak_arrivals;
                    }

                    let mut arrivals = state_unlocked
                        .septa_arrivals
                        .iter()
                        .cloned()
                        .chain(state_unlocked.amtrak_arrivals.iter().cloned())
                        .collect::<Vec<_>>();
                    arrivals.sort_by(|a, b| a.schedule_arrival.cmp(&b.schedule_arrival));

                    state_unlocked.combined_arrivals = arrivals;
                } // drop(state_unlocked)

                select! {
                    _ = tokio::time::sleep_until(refresh_time) => {},
                    _ = task_cancel_token.cancelled() => break,
                }
            }

            Ok(())
        });

        Self {
            state,
            station,
            cancel_token,
            update_task_handle: Some(update_task_handle),
        }
    }
}

const SEPTA_IMAGE: &[u8] = include_bytes!("../../../assets/SEPTA_16.bmp");

lazy_static! {
    static ref SEPTA_BMP: Bmp::<'static, Rgb888> = Bmp::<Rgb888>::from_slice(SEPTA_IMAGE).unwrap();
}

type UpcomingArrivalViews<'a, C> = chain! {
    Text<'a, MonoTextStyle<'static, C>>,
    Text<'a, MonoTextStyle<'static, C>>,
    Text<'a, MonoTextStyle<'static, C>>,
    Text<'a, MonoTextStyle<'static, C>>
};

#[derive(ViewGroup)]
enum LayoutView<'a, C: PixelColor> {
    UpcomingArrival(
        LinearLayout<
            Horizontal<vertical::Center, spacing::FixedMargin>,
            UpcomingArrivalViews<'a, C>,
        >,
    ),
    NoArrival(
        LinearLayout<
            Horizontal<vertical::Center, spacing::FixedMargin>,
            chain! { Text<'a, MonoTextStyle<'static, C>> },
        >,
    ),
}

impl<D> Render<D> for UpcomingArrivals
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<(), D::Error> {
        let station_name = self.station.to_string();

        let canvas_bounding_box = canvas.bounding_box();
        let mut remaining_height = canvas_bounding_box.size.height;

        // Generate the title layout
        let title_layout = LinearLayout::horizontal(
            Chain::new(Image::new(&*SEPTA_BMP, Point::zero())).append(Text::new(
                &station_name,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_9X15, Rgb888::WHITE),
            )),
        )
        .with_alignment(vertical::Center)
        .with_spacing(spacing::FixedMargin(6))
        .arrange();

        remaining_height -= title_layout.bounds().size.height;

        let mut arrival_layouts = Vec::new();

        let display_items = self
            .state
            .lock()
            .combined_arrivals
            .iter()
            .map(|arrival| {
                (
                    arrival.schedule_arrival.format("%_H:%M").to_string(),
                    format!("{:<8}", arrival.train_id),
                    format!("{:<20}", arrival.destination_name),
                    match arrival.status {
                        UpcomingTrainStatus::OnTime => "On Time".to_string(),
                        UpcomingTrainStatus::Early(mins) => format!("{} mins early", mins),
                        UpcomingTrainStatus::Late(mins) => format!("{} mins late", mins),
                        UpcomingTrainStatus::Unknown => "N/A".to_string(),
                    },
                    match arrival.status {
                        UpcomingTrainStatus::OnTime | UpcomingTrainStatus::Early(_) => {
                            Rgb888::GREEN
                        }
                        UpcomingTrainStatus::Late(_) => Rgb888::RED,
                        UpcomingTrainStatus::Unknown => Rgb888::WHITE,
                    },
                )
            })
            .collect::<Vec<_>>();

        if display_items.is_empty() {
            arrival_layouts.push(LayoutView::NoArrival(
                LinearLayout::horizontal(Chain::new(Text::new(
                    "No upcoming arrivals",
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_6X9, Rgb888::WHITE),
                )))
                .with_alignment(vertical::Center)
                .with_spacing(spacing::FixedMargin(6))
                .arrange(),
            ));
        } else {
            for display_item in &display_items {
                let (time, train_id, destination_name, status, status_color) = display_item;

                let chain = Chain::new(Text::new(
                    time,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
                ))
                .append(Text::new(
                    train_id,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
                ))
                .append(Text::new(
                    destination_name,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
                ))
                .append(Text::new(
                    status,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, *status_color),
                ));

                let chain_height = chain.bounds().size.height;

                if remaining_height < chain_height {
                    break;
                }

                remaining_height -= chain.bounds().size.height;

                arrival_layouts.push(LayoutView::UpcomingArrival(
                    LinearLayout::horizontal(chain)
                        .with_alignment(vertical::Center)
                        .with_spacing(spacing::FixedMargin(6))
                        .arrange(),
                ));
            }
        }

        LinearLayout::vertical(
            Chain::new(title_layout).append(
                LinearLayout::vertical(Views::new(arrival_layouts.as_mut_slice()))
                    .with_spacing(spacing::FixedMargin(3))
                    .arrange(),
            ),
        )
        .with_spacing(spacing::FixedMargin(2))
        .arrange()
        .draw(canvas)?;

        Ok(())
    }
}

impl Drop for UpcomingArrivals {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.update_task_handle.take() {
            task_handle.abort();
        }
    }
}

pub struct UpcomingArrivalsFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    _phantom: PhantomData<D>,
}

impl<D> Default for UpcomingArrivalsFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<D> RenderFactory<D> for UpcomingArrivalsFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render_name(&self) -> &'static str {
        "UpcomingArrivals"
    }

    fn render_description(&self) -> &'static str {
        "Upcoming train arrivals for SEPTA regional rail and Amtrak"
    }

    fn load_from_config<R: Read>(&self, reader: R) -> Result<Box<dyn Render<D>>> {
        let config: UpcomingArrivalsConfig = serde_json::from_reader(reader)?;
        Ok(Box::new(UpcomingArrivals::new(config)))
    }
}
