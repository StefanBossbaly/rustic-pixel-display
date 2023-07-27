use anyhow::{anyhow, Result};
use embedded_graphics::{
    mono_font::{self, MonoTextStyle},
    pixelcolor::{Rgb555, Rgb565, Rgb888},
    prelude::{DrawTarget, PixelColor, Point, RgbColor},
    text::Text,
    Drawable,
};
use embedded_layout::{
    chain,
    layout::linear::{spacing, Horizontal, LinearLayout},
    prelude::{horizontal, vertical, Chain, Link},
    View,
};
use embedded_layout_macros::ViewGroup;
use geoutils::{Distance, Location};
use log::{debug, error};
use parking_lot::Mutex;
use rustic_pixel_display::render::{Render, RenderFactory, SubCanvas};
use septa_api::{responses::Train, types::RegionalRailStop};
use serde::Deserialize;
use std::{
    collections::HashMap,
    convert::Infallible,
    io::Read,
    marker::PhantomData,
    sync::Arc,
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
use tinybmp::Bmp;
use tokio::{join, select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use super::{State, StateProvider, SubRender, Usefulness};

/// The amount of time the user has to be within the radius of a station to be considered at the station.
const NO_STATUS_TO_AT_STATION: Duration = Duration::from_secs(30);

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    /// The radius around a station that a user must be within to be considered at the station.
    static ref AT_STATION_ENTER_RADIUS: Distance = Distance::from_meters(200.0);
}

/// The amount of time that a user would need to be outside a station's radius to
/// transition from AtStation to NoStatus.
const AT_STATION_TO_NO_STATUS_TIMEOUT: Duration = Duration::from_secs(60);

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    static ref AT_STATION_LEAVE_RADIUS: Distance = Distance::from_meters(200.0);
}

const ON_TRAIN_TO_NO_STATUS_TIMEOUT: Duration = Duration::from_secs(300);

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    static ref ON_TRAIN_ENTER_RADIUS: Distance = Distance::from_meters(400.0);
    static ref ON_TRAIN_REMAIN_RADIUS: Distance = Distance::from_meters(400.0);
}

const SEPTA_IMAGE: &[u8] = include_bytes!("../../../assets/SEPTA_16.bmp");

lazy_static! {
    static ref SEPTA_BMP: Bmp::<'static, Rgb888> = Bmp::<Rgb888>::from_slice(SEPTA_IMAGE).unwrap();
}

#[derive(Debug, Default, Clone)]
struct TrainEncounter {
    /// The first time the user encountered the train inside the radius of the current station.
    first_encounter_inside_station: Option<Instant>,

    /// The first time the user encountered the train outside the radius of the current station.
    first_encounter_outside_station: Option<Instant>,
}

#[derive(Debug, Clone)]
enum TransitState {
    NoStatus {
        /// A map of the regional rail stop to the time the user entered into the radius of the station.
        /// We use time::Instance since we need a monotonic clock and do not care about the system time.
        station_to_first_encounter: HashMap<RegionalRailStop, Instant>,
    },
    AtStation {
        /// The station the user is currently at.
        station: RegionalRailStop,

        /// A map from the unique train id to the time the user encountered the train within the radius
        /// of a station and the time the user encountered the train outside the radius of a station.
        train_id_to_first_encounter: HashMap<String, TrainEncounter>,

        /// The time the user has been outside the radius of the station.
        time_outside_station: Option<Instant>,
    },
    OnTrain {
        /// The train (wrap in Box to get rid of the clippy::large_enum_variant lint warning)
        train: Box<Train>,

        /// The time the user has been on the train.
        last_train_encounter: Instant,
    },
}

impl Default for TransitState {
    fn default() -> Self {
        Self::NoStatus {
            station_to_first_encounter: HashMap::new(),
        }
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct TransitTrackerConfig {
    pub home_assistant_url: String,
    pub home_assistant_bearer_token: String,
    pub person_entity_id: String,
}

impl TransitState {
    fn new() -> Self {
        Self::default()
    }

    fn update_state(self, lat_lon: (f64, f64), trains: Vec<Train>) -> Result<Self> {
        // Get the monotonic time
        let now = Instant::now();
        let person_location = Location::new(lat_lon.0, lat_lon.1);

        Ok(match self {
            TransitState::NoStatus {
                mut station_to_first_encounter,
            } => {
                let mut eligible_stations = Vec::new();

                // See if we are currently in any station's radius
                for station in
                    RegionalRailStop::iter().filter(|p| !matches!(p, RegionalRailStop::Unknown(_)))
                {
                    let station_lat_lon = station.lat_lon()?;
                    let station_location = Location::new(station_lat_lon.0, station_lat_lon.1);

                    if person_location
                        .is_in_circle(&station_location, *AT_STATION_ENTER_RADIUS)
                        .expect("is_in_circle failed")
                    {
                        match station_to_first_encounter.get(&station) {
                            Some(first_encounter) => {
                                if now - *first_encounter > NO_STATUS_TO_AT_STATION {
                                    eligible_stations.push(station);
                                }
                            }
                            None => {
                                station_to_first_encounter.insert(station.clone(), now);
                            }
                        }
                    } else {
                        // We are not in the radius of the station, so remove it from the map
                        station_to_first_encounter.remove(&station);
                    }
                }

                // Iterate over eligible stations, if there are more than one, pick the closest one
                match eligible_stations.len() {
                    0 => TransitState::NoStatus {
                        station_to_first_encounter,
                    },
                    1 => {
                        let station = eligible_stations[0].clone();

                        debug!(
                            "Transitioning from NoStatus to AtStation (station: {})",
                            station.to_string()
                        );
                        TransitState::AtStation {
                            station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        }
                    }
                    _ => {
                        let mut closest_station = eligible_stations[0].clone();
                        let closest_lat_lon = closest_station.lat_lon()?;
                        let mut closest_distance: Distance = person_location
                            .distance_to(&Location::new(closest_lat_lon.0, closest_lat_lon.1))
                            .map_err(|e| anyhow!("distance_to failed: {}", e))?;

                        for station in eligible_stations {
                            let station_lat_lon = station.lat_lon()?;
                            let distance = person_location
                                .distance_to(&Location::new(station_lat_lon.0, station_lat_lon.1))
                                .map_err(|e| anyhow!("distance_to failed: {}", e))?;

                            if distance.meters() < closest_distance.meters() {
                                closest_station = station;
                                closest_distance = distance;
                            }
                        }

                        debug!(
                            "Transitioning from NoStatus to AtStation for (station: {})",
                            closest_station.to_string()
                        );
                        TransitState::AtStation {
                            station: closest_station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        }
                    }
                }
            }
            TransitState::AtStation {
                station,
                mut train_id_to_first_encounter,
                mut time_outside_station,
            } => {
                let station_location = {
                    let station_lat_lon = station.lat_lon()?;
                    Location::new(station_lat_lon.0, station_lat_lon.1)
                };

                // See if we are still at the current location
                let mut is_outside_location = false;
                if person_location
                    .is_in_circle(&station_location, *AT_STATION_LEAVE_RADIUS)
                    .map_err(|e| anyhow!("distance_to failed: {}", e))?
                {
                    // We are still at the station, so update the time we have been outside the station
                    time_outside_station = None;
                } else {
                    // We are no longer at the station, so update the time we have been outside the station
                    match time_outside_station {
                        Some(first_left) => {
                            if now - first_left > AT_STATION_TO_NO_STATUS_TIMEOUT {
                                is_outside_location = true;
                            }
                        }
                        None => {
                            time_outside_station = Some(now);
                        }
                    }
                }

                if is_outside_location {
                    debug!(
                        "Transitioning from AtStation to NoStatus (station: {})",
                        station.to_string()
                    );
                    TransitState::default()
                } else {
                    // See if we are in the radius of any train
                    let mut matched_train = None;
                    'train_loop: for train in trains {
                        let train_location = Location::new(train.lat, train.lon);
                        if person_location
                            .is_in_circle(&train_location, *ON_TRAIN_ENTER_RADIUS)
                            .map_err(|e| anyhow!("distance_to failed: {}", e))?
                        {
                            match train_id_to_first_encounter.get_mut(&train.train_number) {
                                Some(train_encounters) => {
                                    let currently_at_station = person_location
                                        .is_in_circle(&train_location, *AT_STATION_LEAVE_RADIUS)
                                        .map_err(|e| anyhow!("distance_to failed: {}", e))?;

                                    if currently_at_station {
                                        train_encounters.first_encounter_inside_station =
                                            train_encounters
                                                .first_encounter_inside_station
                                                .or(Some(now));
                                    } else {
                                        train_encounters.first_encounter_outside_station =
                                            train_encounters
                                                .first_encounter_outside_station
                                                .or(Some(now));
                                    }

                                    // We have to have at least one encounter inside the station and one outside the station
                                    // TODO: Have some sort of time component to this transition
                                    if train_encounters.first_encounter_inside_station.is_some()
                                        && train_encounters
                                            .first_encounter_outside_station
                                            .is_some()
                                    {
                                        matched_train = Some(train);
                                        break 'train_loop;
                                    }
                                }
                                None => {
                                    let mut train_encounters = TrainEncounter {
                                        first_encounter_inside_station: None,
                                        first_encounter_outside_station: None,
                                    };

                                    let currently_at_station = person_location
                                        .is_in_circle(&train_location, *AT_STATION_LEAVE_RADIUS)
                                        .map_err(|e| anyhow!("distance_to failed: {}", e))?;

                                    if currently_at_station {
                                        train_encounters.first_encounter_inside_station =
                                            train_encounters
                                                .first_encounter_inside_station
                                                .or(Some(now));
                                    } else {
                                        train_encounters.first_encounter_outside_station =
                                            train_encounters
                                                .first_encounter_outside_station
                                                .or(Some(now));
                                    }

                                    train_id_to_first_encounter
                                        .insert(train.train_number, train_encounters);
                                }
                            }
                        } else {
                            // We are not in the radius of the train, so remove it from the map
                            train_id_to_first_encounter.remove(&train.train_number);
                        }
                    }

                    match matched_train {
                        Some(train) => {
                            debug!(
                                "Transitioning from AtStation to OnTrain (station: {}, train: {})",
                                station.to_string(),
                                train.train_number
                            );
                            TransitState::OnTrain {
                                train: Box::new(train),
                                last_train_encounter: now,
                            }
                        }
                        None => TransitState::AtStation {
                            station,
                            train_id_to_first_encounter,
                            time_outside_station,
                        },
                    }
                }
            }
            TransitState::OnTrain {
                train,
                mut last_train_encounter,
                ..
            } => {
                // See if we are still in the radius of the train
                let current_train = trains
                    .into_iter()
                    .find(|train_itr| train_itr.train_number == train.train_number);

                match current_train {
                    Some(train) => {
                        let train_location = Location::new(train.lat, train.lon);
                        if person_location
                            .is_in_circle(&train_location, *ON_TRAIN_REMAIN_RADIUS)
                            .map_err(|e| anyhow!("distance_to failed: {}", e))?
                        {
                            last_train_encounter = now;
                            TransitState::OnTrain {
                                train: Box::new(train),
                                last_train_encounter,
                            }
                        } else if last_train_encounter - now > ON_TRAIN_TO_NO_STATUS_TIMEOUT {
                            let station: Option<RegionalRailStop> = {
                                let mut regional_rail_stop = None;
                                for station in RegionalRailStop::iter() {
                                    let station_location = {
                                        let (lat, lon) = station.lat_lon()?;
                                        Location::new(lat, lon)
                                    };
                                    if person_location
                                        .is_in_circle(&station_location, *AT_STATION_ENTER_RADIUS)
                                        .map_err(|e| anyhow!("distance_to failed: {}", e))?
                                    {
                                        regional_rail_stop = Some(station);
                                        break;
                                    }
                                }

                                regional_rail_stop
                            };

                            match station {
                                Some(station) => {
                                    debug!(
                                        "Transitioning from OnTrain to AtStation (station: {}, train: {})",
                                        station.to_string(), train.train_number
                                    );
                                    TransitState::AtStation {
                                        station,
                                        time_outside_station: None,
                                        train_id_to_first_encounter: HashMap::new(),
                                    }
                                }
                                None => {
                                    debug!(
                                        "Transitioning from OnTrain to NoStatus (train: {})",
                                        train.train_number
                                    );
                                    TransitState::default()
                                }
                            }
                        } else {
                            TransitState::OnTrain {
                                train: Box::new(train),
                                last_train_encounter,
                            }
                        }
                    }
                    None => TransitState::default(),
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TrainStatus {
    Early(i32),
    OnTime,
    Late(i32),
}

#[derive(Clone, Debug)]
pub enum DisplayTransitState {
    NoStatus,
    AtStation {
        station_name: String,
    },
    OnTrain {
        train_number: String,
        status: TrainStatus,
        status_text: String,
        destination: String,
    },
}

impl Usefulness for DisplayTransitState {
    fn usefulness(&self) -> super::UsefulnessVal {
        match self {
            DisplayTransitState::NoStatus => super::UsefulnessVal::NotUseful,
            DisplayTransitState::AtStation { .. } => super::UsefulnessVal::VeryUseful,
            DisplayTransitState::OnTrain { .. } => super::UsefulnessVal::VeryUseful,
        }
    }
}

impl<D> SubRender<D> for DisplayTransitState
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn sub_render(&self, sub_canvas: &mut SubCanvas<D>) -> Result<()> {
        // Attempt to figure out the transit state
        let status_view = match self {
            DisplayTransitState::NoStatus => {
                let chain = Chain::new(Text::new(
                    "No Status",
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
                ));

                PersonStatusView::NoStatus(
                    LinearLayout::horizontal(chain)
                        .with_alignment(vertical::Center)
                        .with_spacing(spacing::FixedMargin(6))
                        .arrange(),
                )
            }
            DisplayTransitState::AtStation { station_name } => {
                let chain = Chain::new(Text::new(
                    station_name,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
                ));

                PersonStatusView::AtStation(
                    LinearLayout::horizontal(chain)
                        .with_alignment(vertical::Center)
                        .with_spacing(spacing::FixedMargin(6))
                        .arrange(),
                )
            }
            DisplayTransitState::OnTrain {
                train_number,
                status,
                status_text,
                destination,
            } => {
                let status_color = match status {
                    TrainStatus::Early(_) | TrainStatus::OnTime => Rgb888::GREEN,
                    TrainStatus::Late(_) => Rgb888::RED,
                };

                let chain = Chain::new(Text::new(
                    train_number,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
                ))
                .append(Text::new(
                    status_text,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_6X10, status_color),
                ))
                .append(Text::new(
                    destination,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
                ));

                PersonStatusView::OnTrain(
                    LinearLayout::horizontal(chain)
                        .with_alignment(vertical::Center)
                        .with_spacing(spacing::FixedMargin(6))
                        .arrange(),
                )
            }
        };

        LinearLayout::vertical(Chain::new(status_view))
            .with_alignment(horizontal::Left)
            .with_spacing(spacing::FixedMargin(4))
            .arrange()
            .draw(sub_canvas)
            .unwrap();

        Ok(())
    }
}

impl From<&TransitState> for DisplayTransitState {
    fn from(value: &TransitState) -> Self {
        match value {
            TransitState::NoStatus { .. } => Self::NoStatus,
            TransitState::AtStation { station, .. } => Self::AtStation {
                station_name: station.to_string(),
            },
            TransitState::OnTrain { train, .. } => Self::OnTrain {
                train_number: train.train_number.clone(),
                status: match train.late.cmp(&0) {
                    std::cmp::Ordering::Less => TrainStatus::Early(train.late),
                    std::cmp::Ordering::Equal => TrainStatus::OnTime,
                    std::cmp::Ordering::Greater => TrainStatus::Late(-train.late),
                },
                status_text: match train.late.cmp(&0) {
                    std::cmp::Ordering::Less => format!("{} Mins Early", train.late),
                    std::cmp::Ordering::Equal => "On Time".to_string(),
                    std::cmp::Ordering::Greater => format!("{} Mins Late", -train.late),
                },
                destination: train.dest.to_string(),
            },
        }
    }
}

pub struct TransitTracker {
    state: Arc<Mutex<TransitState>>,

    /// Used to signal that all async tasks should be cancelled immediately
    cancel_token: CancellationToken,

    /// Handle to the task used to update the SEPTA and User location
    update_task_handle: Option<JoinHandle<Result<()>>>,
}

impl TransitTracker {
    async fn get_location(
        home_assistant_client: &home_assistant_rest::Client,
        config: &TransitTrackerConfig,
    ) -> Result<(f64, f64)> {
        let entity_state = home_assistant_client
            .get_states_of_entity(&config.person_entity_id)
            .await?;

        if let (Some(lat), Some(lon)) = (
            entity_state.attributes.get("latitude"),
            entity_state.attributes.get("longitude"),
        ) {
            if let (Some(lat_f64), Some(lon_f64)) = (lat.as_f64(), lon.as_f64()) {
                Ok((lat_f64, lon_f64))
            } else {
                Err(anyhow!("Could not match lat lng"))
            }
        } else {
            Err(anyhow!("Could not match lat lng"))
        }
    }

    pub fn new(config: TransitTrackerConfig) -> Result<Self> {
        let septa_client = septa_api::Client::new();
        let home_assistant_client = home_assistant_rest::Client::new(
            &config.home_assistant_url,
            &config.home_assistant_bearer_token,
        )?;

        let state_holder = Arc::new(Mutex::new(TransitState::new()));
        let cancel_token = CancellationToken::new();

        // Clone the shared data since it will be moved onto the task
        let task_state_holder = state_holder.clone();
        let task_cancel_token = cancel_token.clone();

        let update_task_handle: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            'update_loop: loop {
                let refresh_time = tokio::time::Instant::now() + Duration::from_secs(15);

                let trains_request = septa_client.train_view();
                let user_location_request = Self::get_location(&home_assistant_client, &config);

                let (trains_result, user_location_result) =
                    join!(trains_request, user_location_request);

                match (user_location_result, trains_result) {
                    (Ok((user_loc_lat, user_loc_lon)), Ok(trains)) => {
                        let mut holder_unlocked = task_state_holder.lock();

                        let transit_state = std::mem::take(&mut *holder_unlocked);
                        let new_state =
                            transit_state.update_state((user_loc_lat, user_loc_lon), trains)?;

                        debug!("Updated state: {:?}", new_state);

                        let _ = std::mem::replace(&mut *holder_unlocked, new_state);
                    }
                    (Err(location_error), Err(train_error)) => {
                        error!("Error in both location and SEPTA calls (location_error: {location_error}, train_error: {train_error})");
                    }
                    (Ok(_), Err(train_error)) => {
                        error!("Error in SEPTA call ({train_error})");
                    }
                    (Err(location_error), Ok(_)) => {
                        error!("Error in location call ({location_error})");
                    }
                }

                select! {
                    _ = tokio::time::sleep_until(refresh_time) => {},
                    _ = task_cancel_token.cancelled() => break 'update_loop,
                }
            }

            Ok(())
        });

        Ok(Self {
            state: state_holder,
            cancel_token,
            update_task_handle: Some(update_task_handle),
        })
    }
}

impl<D> StateProvider<D> for TransitTracker
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn provide_state(&self) -> Box<dyn super::State<D>> {
        let display_state: DisplayTransitState = (&*self.state.lock()).into();
        let state: Box<dyn State<D>> = Box::new(display_state);
        state
    }
}

type NoStatusViews<'a, C> = chain! {
    Text<'a, MonoTextStyle<'static, C>>
};

type AtStationViews<'a, C> = chain! {
    Text<'a, MonoTextStyle<'static, C>>
};

type OnTrainViews<'a, C> = chain! {
    Text<'a, MonoTextStyle<'static, C>>,
    Text<'a, MonoTextStyle<'static, C>>,
    Text<'a, MonoTextStyle<'static, C>>
};

#[derive(ViewGroup)]
enum PersonStatusView<'a, C: PixelColor + From<Rgb555> + From<Rgb565> + From<Rgb888>> {
    NoStatus(
        LinearLayout<Horizontal<vertical::Center, spacing::FixedMargin>, NoStatusViews<'a, C>>,
    ),
    AtStation(
        LinearLayout<Horizontal<vertical::Center, spacing::FixedMargin>, AtStationViews<'a, C>>,
    ),
    OnTrain(LinearLayout<Horizontal<vertical::Center, spacing::FixedMargin>, OnTrainViews<'a, C>>),
}

pub struct TransitTrackerFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    _phantom: PhantomData<D>,
}

impl<D> Default for TransitTrackerFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<D> RenderFactory<D> for TransitTrackerFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render_name(&self) -> &'static str {
        "TransitTracker"
    }

    fn render_description(&self) -> &'static str {
        "Tracks a person based on the SEPTA transit information"
    }

    fn load_from_config<R: Read>(&self, _reader: R) -> Result<Box<dyn Render<D>>> {
        todo!()
    }
}

impl Drop for TransitTracker {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.update_task_handle.take() {
            task_handle.abort();
        }
    }
}
