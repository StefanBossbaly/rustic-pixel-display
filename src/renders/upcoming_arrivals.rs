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
use log::{error, trace};
use parking_lot::Mutex;
use rustic_pixel_display::render::{Render, RenderFactory};
use septa_api::{requests::ArrivalsRequest, responses::Arrivals, types::RegionalRailStop};
use serde::Deserialize;
use std::{convert::Infallible, io::Read, marker::PhantomData, sync::Arc, time::Duration};
use tinybmp::Bmp;
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
struct UpcomingTrainsState {
    arrivals: Vec<Arrivals>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpcomingArrivalsConfig {
    pub station: RegionalRailStop,
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

const SEPTA_IMAGE: &[u8] = include_bytes!("../../assets/SEPTA_16.bmp");

lazy_static! {
    static ref SEPTA_BMP: Bmp::<'static, Rgb888> = Bmp::<Rgb888>::from_slice(SEPTA_IMAGE).unwrap();
}

impl UpcomingArrivals {
    pub fn new(config: UpcomingArrivalsConfig) -> Self {
        let septa_api = septa_api::Client::new();

        let state = Arc::new(Mutex::new(UpcomingTrainsState::default()));
        let cancel_token = CancellationToken::new();

        let task_cancel_token = cancel_token.clone();
        let task_state = state.clone();
        let task_station = config.station.clone();
        let task_results = config.results;

        let update_task_handle: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            loop {
                let refresh_time = tokio::time::Instant::now() + Duration::from_secs(60);

                match septa_api
                    .arrivals(ArrivalsRequest {
                        station: task_station.clone(),
                        results: task_results,
                        direction: None,
                    })
                    .await
                {
                    Ok(response) => {
                        trace!(
                            "northbound: {:?}, southbound: {:?}",
                            response.northbound,
                            response.southbound
                        );

                        // Sort the arrivals
                        let mut arrivals = Vec::new();
                        arrivals.extend(response.northbound.into_iter());
                        arrivals.extend(response.southbound.into_iter());
                        arrivals.sort_by(|a, b| a.sched_time.cmp(&b.sched_time));

                        let mut state_unlocked = task_state.lock();
                        state_unlocked.arrivals = arrivals
                            .into_iter()
                            .take((task_results.unwrap_or(3) * 2) as usize)
                            .collect::<Vec<_>>();
                    }
                    Err(e) => error!("Could not get updated information {e}"),
                }

                select! {
                    _ = tokio::time::sleep_until(refresh_time) => {},
                    _ = task_cancel_token.cancelled() => break,
                }
            }

            Ok(())
        });

        Self {
            state,
            station: config.station,
            cancel_token,
            update_task_handle: Some(update_task_handle),
        }
    }
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
        let state_unlocked = self.state.lock();

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

        let format_strings = state_unlocked
            .arrivals
            .iter()
            .map(|arrival| {
                (
                    arrival.sched_time.format("%_H:%M").to_string(),
                    arrival.destination.to_string(),
                )
            })
            .collect::<Vec<_>>();

        if state_unlocked.arrivals.is_empty() {
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
            for (index, train) in state_unlocked.arrivals.iter().enumerate() {
                let text_color = match train.status.as_str() {
                    "On Time" => Rgb888::GREEN,
                    _ => Rgb888::RED,
                };

                let chain = Chain::new(Text::new(
                    &format_strings[index].0,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
                ))
                .append(Text::new(
                    &format_strings[index].1,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
                ))
                .append(Text::new(
                    &train.train_id,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
                ))
                .append(Text::new(
                    train.status.as_str(),
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_5X7, text_color),
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
        "Upcoming train arrivals for SEPTA regional rail"
    }

    fn load_from_config<R: Read>(&self, reader: R) -> Result<Box<dyn Render<D>>> {
        let config: UpcomingArrivalsConfig = serde_json::from_reader(reader)?;
        Ok(Box::new(UpcomingArrivals::new(config)))
    }
}
