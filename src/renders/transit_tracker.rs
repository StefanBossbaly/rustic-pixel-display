use crate::render::{Configurable, Render};
use anyhow::{anyhow, Result};
use embedded_graphics::{
    image::Image,
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
use home_assistant_rest::get::StateEnum;
use log::{debug, error, warn};
use parking_lot::Mutex;
use septa_api::{responses::Train, types::RegionalRailStop};
use serde::Deserialize;
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
use tinybmp::Bmp;
use tokio::{join, select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

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

const SEPTA_IMAGE: &[u8] = include_bytes!("../../assets/SEPTA_16.bmp");
const HOME_ICON: &[u8] = include_bytes!("../../assets/home.bmp");
lazy_static! {
    static ref SEPTA_BMP: Bmp::<'static, Rgb888> = Bmp::<Rgb888>::from_slice(SEPTA_IMAGE).unwrap();
    static ref HOME_BMP: Bmp::<'static, Rgb888> = Bmp::<Rgb888>::from_slice(HOME_ICON).unwrap();
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

struct StateHolder {
    /// The current state of the person
    transit_state: TransitState,
    person_name: Option<String>,
    person_state: Option<String>,
}

impl StateHolder {
    fn new() -> Self {
        Self {
            transit_state: TransitState::new(),
            person_name: None,
            person_state: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum TrainStatus {
    Early(i32),
    OnTime,
    Late(i32),
}

#[derive(Clone, Debug)]
enum DisplayTransitState {
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
    state: Arc<Mutex<StateHolder>>,

    /// Used to signal that all async tasks should be cancelled immediately
    cancel_token: CancellationToken,

    /// Handle to the task used to update the SEPTA and User location
    update_task_handle: Option<JoinHandle<Result<()>>>,
}

impl TransitTracker {
    async fn get_location(
        home_assistant_client: &home_assistant_rest::Client,
        config: &TransitTrackerConfig,
    ) -> Result<(Option<String>, Option<String>, f64, f64)> {
        let entity_state = home_assistant_client
            .get_states_of_entity(&config.person_entity_id)
            .await?;

        // Attempt to get the person's name
        let person_name = if let Some(state_value) = entity_state.attributes.get("friendly_name") {
            if let Some(value) = state_value.as_str() {
                Some(value.to_owned())
            } else {
                warn!("Could not parse 'friendly_name' as str");
                None
            }
        } else {
            warn!("Could find 'friendly_name' in attributes");
            None
        };

        // Attempt to get the person's state
        let person_state = if let Some(state_value) = entity_state.state {
            if let StateEnum::String(value) = state_value {
                Some(value)
            } else {
                warn!("Could not parse 'state' as str");
                None
            }
        } else {
            warn!("{}'s 'state' was not provided", config.person_entity_id);
            None
        };

        if let (Some(lat), Some(lon)) = (
            entity_state.attributes.get("latitude"),
            entity_state.attributes.get("longitude"),
        ) {
            if let (Some(lat_f64), Some(lon_f64)) = (lat.as_f64(), lon.as_f64()) {
                Ok((person_name, person_state, lat_f64, lon_f64))
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

        let state_holder = Arc::new(Mutex::new(StateHolder::new()));
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
                    (Ok((person_name, person_state, user_loc_lat, user_loc_lon)), Ok(trains)) => {
                        let mut holder_unlocked = task_state_holder.lock();

                        let transit_state = std::mem::take(&mut holder_unlocked.transit_state);
                        let new_state =
                            transit_state.update_state((user_loc_lat, user_loc_lon), trains)?;

                        debug!("Updated state: {:?}", new_state);

                        let _ = std::mem::replace(&mut holder_unlocked.transit_state, new_state);
                        holder_unlocked.person_name = person_name;
                        holder_unlocked.person_state = person_state;
                    }
                    (Err(location_error), Err(train_error)) => {
                        error!("Error in both location and SEPTA calls (location_error: {location_error}, train_error: {train_error})");
                    }
                    (Ok((person_name, person_state, ..)), Err(train_error)) => {
                        error!("Error in SEPTA call ({train_error})");

                        let mut holder_unlocked = task_state_holder.lock();
                        holder_unlocked.person_name = person_name;
                        holder_unlocked.person_state = person_state;
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

type NoStatusViews<'a, C> = chain! {
    Image<'a, Bmp<'static, C>>,
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

impl<D: DrawTarget<Color = Rgb888, Error = Infallible>> Render<D> for TransitTracker {
    fn render(&self, canvas: &mut D) -> Result<()> {
        let state_unlocked = self.state.lock();

        // Attempt to figure out the name of the person
        let name = match &state_unlocked.person_name {
            Some(name) => name,
            None => "Unknown",
        };

        let display_state: DisplayTransitState = (&state_unlocked.transit_state).into();

        // Attempt to figure out the transit state
        let status_view = match &display_state {
            DisplayTransitState::NoStatus => {
                let state = if let Some(state) = &state_unlocked.person_state {
                    state
                } else {
                    "Unknown"
                };

                let chain = Chain::new(Image::new(&*HOME_BMP, Point::zero())).append(Text::new(
                    state,
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

        LinearLayout::vertical(
            Chain::new(
                LinearLayout::horizontal(Chain::new(Text::new(
                    name,
                    Point::zero(),
                    MonoTextStyle::new(&mono_font::ascii::FONT_9X15, Rgb888::WHITE),
                )))
                .with_alignment(vertical::Center)
                .arrange(),
            )
            .append(status_view),
        )
        .with_alignment(horizontal::Left)
        .with_spacing(spacing::FixedMargin(4))
        .arrange()
        .draw(canvas)
        .unwrap();

        Ok(())
    }
}

impl Configurable for TransitTracker {
    type Config = TransitTrackerConfig;

    fn config_name() -> &'static str {
        "transit_tracker"
    }

    fn load_from_config(config: Self::Config) -> Result<Self> {
        TransitTracker::new(config)
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
