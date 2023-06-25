use crate::{config::TransitConfig, render::Render};
use anyhow::{anyhow, Context, Result};
use embedded_graphics::{
    image::Image,
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor},
    text::Text,
    Drawable,
};
use embedded_layout::{
    layout::linear::{spacing, LinearLayout},
    prelude::{horizontal, vertical, Chain},
    view_group::Views,
};
use geoutils::{Distance, Location};
use home_assistant_rest::get::StateEnum;
use log::{debug, error, trace, warn};
use parking_lot::Mutex;
use septa_api::{
    requests::ArrivalsRequest,
    responses::{Arrivals, Train},
    types::RegionalRailStop,
};
use std::{
    collections::HashMap,
    convert::Infallible,
    error::Error,
    fs::File,
    io::BufReader,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
use tinybmp::Bmp;
use tokio::{join, select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// The amount of time the user has to be within the radius of a station to be considered at the station.
const NO_STATUS_TO_AT_STATION: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Clone)]
struct NoStatusTracker {
    /// A map of the regional rail stop to the time the user entered into the radius of the station.
    /// We use time::Instance since we need a monotonic clock and do not care about the system time.
    station_to_first_encounter: HashMap<RegionalRailStop, Instant>,
}

#[derive(Debug, Default, Clone)]
struct TrainEncounter {
    /// The first time the user encountered the train inside the radius of the current station.
    first_encounter_inside_station: Option<Instant>,

    /// The first time the user encountered the train outside the radius of the current station.
    first_encounter_outside_station: Option<Instant>,
}

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

#[derive(Debug, Clone)]
struct StationTracker {
    /// The station the user is currently at.
    station: RegionalRailStop,

    /// A map from the unique train id to the time the user encountered the train within the radius
    /// of a station and the time the user encountered the train outside the radius of a station.
    train_id_to_first_encounter: HashMap<String, TrainEncounter>,

    /// The time the user has been outside the radius of the station.
    time_outside_station: Option<Instant>,
}

const ON_TRAIN_TO_NO_STATUS_TIMEOUT: Duration = Duration::from_secs(300);

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    static ref ON_TRAIN_ENTER_RADIUS: Distance = Distance::from_meters(400.0);
    static ref ON_TRAIN_REMAIN_RADIUS: Distance = Distance::from_meters(400.0);
}

#[derive(Debug, Clone)]
struct TrainTracker {
    /// The unique train id.
    train_id: String,

    /// The time the user has been on the train.
    last_train_encounter: Instant,
}

#[derive(Debug, Clone)]
enum TransitState {
    NoStatus(NoStatusTracker),
    AtStation(StationTracker),
    OnTrain(TrainTracker),
}

impl Default for TransitState {
    fn default() -> Self {
        Self::new()
    }
}

impl TransitState {
    fn new() -> Self {
        TransitState::NoStatus(NoStatusTracker::default())
    }

    fn update_state(self, lat_lon: (f64, f64), trains: Vec<Train>) -> Result<Self> {
        // Get the monotonic time
        let now = Instant::now();
        let person_location = Location::new(lat_lon.0, lat_lon.1);

        Ok(match self {
            TransitState::NoStatus(mut tracker) => {
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
                        match tracker.station_to_first_encounter.get(&station) {
                            Some(first_encounter) => {
                                if now - *first_encounter > NO_STATUS_TO_AT_STATION {
                                    eligible_stations.push(station);
                                }
                            }
                            None => {
                                tracker
                                    .station_to_first_encounter
                                    .insert(station.clone(), now);
                            }
                        }
                    } else {
                        // We are not in the radius of the station, so remove it from the map
                        tracker.station_to_first_encounter.remove(&station);
                    }
                }

                // Iterate over eligible stations, if there are more than one, pick the closest one
                match eligible_stations.len() {
                    0 => TransitState::NoStatus(tracker),
                    1 => {
                        let station = eligible_stations[0].clone();

                        debug!(
                            "Transitioning from NoStatus to AtStation (station: {})",
                            station.to_string()
                        );
                        TransitState::AtStation(StationTracker {
                            station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        })
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
                        TransitState::AtStation(StationTracker {
                            station: closest_station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        })
                    }
                }
            }
            TransitState::AtStation(mut tracker) => {
                let station_location = {
                    let station_lat_lon = tracker.station.lat_lon()?;
                    Location::new(station_lat_lon.0, station_lat_lon.1)
                };

                // See if we are still at the current location
                let mut is_outside_location = false;
                if person_location
                    .is_in_circle(&station_location, *AT_STATION_LEAVE_RADIUS)
                    .map_err(|e| anyhow!("distance_to failed: {}", e))?
                {
                    // We are still at the station, so update the time we have been outside the station
                    tracker.time_outside_station = None;
                } else {
                    // We are no longer at the station, so update the time we have been outside the station
                    match tracker.time_outside_station {
                        Some(first_left) => {
                            if now - first_left > AT_STATION_TO_NO_STATUS_TIMEOUT {
                                is_outside_location = true;
                            }
                        }
                        None => {
                            tracker.time_outside_station = Some(now);
                        }
                    }
                }

                if is_outside_location {
                    debug!(
                        "Transitioning from AtStation to NoStatus (station: {})",
                        tracker.station.to_string()
                    );
                    TransitState::NoStatus(NoStatusTracker::default())
                } else {
                    // See if we are in the radius of any train
                    let mut matched_train = None;
                    'train_loop: for train in trains {
                        let train_location = Location::new(train.lat, train.lon);
                        if person_location
                            .is_in_circle(&train_location, *ON_TRAIN_ENTER_RADIUS)
                            .map_err(|e| anyhow!("distance_to failed: {}", e))?
                        {
                            match tracker
                                .train_id_to_first_encounter
                                .get_mut(&train.train_number)
                            {
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
                                        matched_train = Some(train.train_number);
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

                                    tracker
                                        .train_id_to_first_encounter
                                        .insert(train.train_number, train_encounters);
                                }
                            }
                        } else {
                            // We are not in the radius of the train, so remove it from the map
                            tracker
                                .train_id_to_first_encounter
                                .remove(&train.train_number);
                        }
                    }

                    match matched_train {
                        Some(train_id) => {
                            debug!(
                                "Transitioning from AtStation to OnTrain (station: {}, train: {})",
                                tracker.station.to_string(),
                                train_id
                            );
                            TransitState::OnTrain(TrainTracker {
                                train_id,
                                last_train_encounter: now,
                            })
                        }
                        None => TransitState::AtStation(tracker),
                    }
                }
            }
            TransitState::OnTrain(mut tracker) => {
                // See if we are still in the radius of the train
                let current_train = trains
                    .iter()
                    .find(|&train| train.train_number == tracker.train_id);

                match current_train {
                    Some(train) => {
                        let train_location = Location::new(train.lat, train.lon);
                        if person_location
                            .is_in_circle(&train_location, *ON_TRAIN_REMAIN_RADIUS)
                            .map_err(|e| anyhow!("distance_to failed: {}", e))?
                        {
                            tracker.last_train_encounter = now;
                            TransitState::OnTrain(tracker)
                        } else if tracker.last_train_encounter - now > ON_TRAIN_TO_NO_STATUS_TIMEOUT
                        {
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
                                        station.to_string(), tracker.train_id
                                    );
                                    TransitState::AtStation(StationTracker {
                                        station,
                                        time_outside_station: None,
                                        train_id_to_first_encounter: HashMap::new(),
                                    })
                                }
                                None => {
                                    debug!(
                                        "Transitioning from OnTrain to NoStatus (train: {})",
                                        tracker.train_id
                                    );
                                    TransitState::NoStatus(NoStatusTracker::default())
                                }
                            }
                        } else {
                            TransitState::OnTrain(tracker)
                        }
                    }
                    None => TransitState::NoStatus(NoStatusTracker {
                        station_to_first_encounter: HashMap::new(),
                    }),
                }
            }
        })
    }
}

struct StateHolder {
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

enum SimpleTransitState {
    NoStatus,
    AtStation(RegionalRailStop),
    OnTrain(String),
}

impl From<&TransitState> for SimpleTransitState {
    fn from(value: &TransitState) -> Self {
        match value {
            TransitState::NoStatus(_) => Self::NoStatus,
            TransitState::AtStation(station_tracker) => {
                Self::AtStation(station_tracker.station.clone())
            }
            TransitState::OnTrain(train_tracker) => Self::OnTrain(train_tracker.train_id.clone()),
        }
    }
}

enum SupplementalTransitInfo {
    AtStation(
        Option<septa_api::responses::Arrivals>,
        Option<septa_api::responses::Arrivals>,
    ),
}

pub(crate) struct TransitRender {
    state: Arc<Mutex<StateHolder>>,

    cancel_token: CancellationToken,

    /// Handle to the task used to update the SEPTA and User location
    update_task_handle: Option<JoinHandle<Result<()>>>,

    /// We provide more information to the render based on what state we are in
    supplement_transit_info: Arc<Mutex<Option<SupplementalTransitInfo>>>,
}

impl TransitRender {
    const CONFIG_FILE: &'static str = "transit.yaml";
    const SEPTA_IMAGE_BIG: &[u8] = include_bytes!("../assets/SEPTA.bmp");

    fn get_config_file() -> Result<File> {
        let home_dir = std::env::var("HOME").context("Can not load HOME environment variable")?;
        let mut file_path = PathBuf::from(home_dir);
        file_path.push(Self::CONFIG_FILE);
        File::open(file_path).with_context(|| format!("Failed to open file {}", Self::CONFIG_FILE))
    }

    fn read_config() -> Result<TransitConfig> {
        let file_reader = BufReader::new(Self::get_config_file()?);
        serde_yaml::from_reader(file_reader).context("Unable to parse YAML file")
    }

    async fn get_location(
        home_assistant_client: &home_assistant_rest::Client,
        config: &TransitConfig,
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

    pub(crate) fn new(config: TransitConfig) -> Result<Self, Box<dyn Error>> {
        let septa_client = septa_api::Client::new();
        let home_assistant_client = home_assistant_rest::Client::new(
            &config.home_assistant_url,
            &config.home_assistant_bearer_token,
        )?;

        let state_holder = Arc::new(Mutex::new(StateHolder::new()));
        let supplement_transit_info = Arc::new(Mutex::new(None));
        let cancel_token = CancellationToken::new();

        // Clone the shared data since it will be moved onto the task
        let task_state_holder = state_holder.clone();
        let task_supplement_transit_info = supplement_transit_info.clone();
        let task_cancel_token = cancel_token.clone();

        let update_task_handle: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            loop {
                let trains_request = septa_client.train_view();
                let user_location_request = Self::get_location(&home_assistant_client, &config);

                let (trains_result, user_location_result) =
                    join!(trains_request, user_location_request);

                let (person_name, person_state, user_loc_lat, user_loc_lon) = user_location_result?;
                let trains = trains_result?;

                let mut simple_state: Option<SimpleTransitState>;

                {
                    let mut holder_unlocked = task_state_holder.lock();

                    let transit_state = std::mem::take(&mut holder_unlocked.transit_state);
                    let new_state =
                        transit_state.update_state((user_loc_lat, user_loc_lon), trains)?;

                    simple_state = Some((&holder_unlocked.transit_state).into());

                    let _ = std::mem::replace(&mut holder_unlocked.transit_state, new_state);
                    holder_unlocked.person_name = person_name;
                    holder_unlocked.person_state = person_state;
                } // drop(holder_unlocked)

                // Provide supplemental information
                match simple_state.take() {
                    Some(state) => match state {
                        SimpleTransitState::NoStatus => {
                            *task_supplement_transit_info.lock() = None;
                        }
                        SimpleTransitState::AtStation(stop) => {
                            match septa_client
                                .arrivals(septa_api::requests::ArrivalsRequest {
                                    station: stop,
                                    results: Some(1),
                                    direction: None,
                                })
                                .await
                            {
                                Ok(arrivals) => {
                                    *task_supplement_transit_info.lock() =
                                        Some(SupplementalTransitInfo::AtStation(
                                            arrivals.northbound.get(0).cloned(),
                                            arrivals.southbound.get(0).cloned(),
                                        ));
                                }
                                Err(e) => error!("Error occurred while getting arrival data: {e}"),
                            }
                        }
                        SimpleTransitState::OnTrain(_) => {
                            *task_supplement_transit_info.lock() = None;
                        }
                    },
                    None => error!("Simple state was not updated!"),
                }

                select! {
                    _ = tokio::time::sleep(Duration::from_secs(15)) => {},
                    _ = task_cancel_token.cancelled() => break,
                }
            }

            Ok(())
        });

        Ok(Self {
            state: state_holder,
            cancel_token,
            update_task_handle: Some(update_task_handle),
            supplement_transit_info,
        })
    }

    pub(crate) fn from_config() -> Result<Self, Box<dyn Error>> {
        Self::new(Self::read_config()?)
    }
}

impl<D: DrawTarget<Color = Rgb888, Error = Infallible>> Render<D> for TransitRender {
    fn render(&self, canvas: &mut D) -> Result<()> {
        trace!("Render called");
        let state_unlocked = self.state.lock();

        // Attempt to figure out the name of the person
        let name = match &state_unlocked.person_name {
            Some(name) => name,
            None => "Unknown",
        };

        // Attempt to figure out the transit state
        let status_text = match &state_unlocked.transit_state {
            TransitState::NoStatus(_) => {
                if let Some(state) = &state_unlocked.person_state {
                    state.to_owned()
                } else {
                    "Unknown".to_owned()
                }
            }
            TransitState::AtStation(tracker) => {
                format!("At Station {}", tracker.station)
            }
            TransitState::OnTrain(tracker) => format!("On Train {}", tracker.train_id),
        };

        let supplement_transit_info_unlocked = self.supplement_transit_info.lock();

        let mut northbound_status = None;

        if let Some(info) = &*supplement_transit_info_unlocked {
            match info {
                SupplementalTransitInfo::AtStation(northbound, _) => {
                    if let Some(northbound) = northbound {
                        northbound_status = Some((
                            format!("Train {}", northbound.train_id),
                            format!("Status: {}", northbound.status),
                        ));
                    }
                }
            }
        }

        let mut supplemental_views = Vec::new();

        if let Some(north) = &northbound_status {
            supplemental_views.push(Text::new(
                &north.0,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ));
            supplemental_views.push(Text::new(
                &north.1,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::RED),
            ));
        } else {
            supplemental_views.push(Text::new(
                "No Trains",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ));
        }

        LinearLayout::vertical(
            Chain::new(Text::new(
                name,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
            ))
            .append(Text::new(
                status_text.as_str(),
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ))
            .append(
                LinearLayout::horizontal(Views::new(supplemental_views.as_mut_slice())).arrange(),
            ),
        )
        .with_alignment(horizontal::Left)
        .arrange()
        .draw(canvas)
        .unwrap();

        Image::new(
            &Bmp::<Rgb888>::from_slice(Self::SEPTA_IMAGE_BIG).unwrap(),
            Point::new(40, 20),
        )
        .draw(canvas)?;

        Ok(())
    }
}

impl Drop for TransitRender {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.update_task_handle.take() {
            task_handle.abort();
        }
    }
}

#[derive(Debug, Default)]
struct UpcomingTrainsState {
    southbound: Vec<Arrivals>,
    northbound: Vec<Arrivals>,
}

pub(crate) struct UpcomingTrainsRender {
    state: Arc<Mutex<UpcomingTrainsState>>,

    /// The regional rail stop
    station: RegionalRailStop,

    /// Flag used to gracefully terminate the render and driver threads
    cancel_token: CancellationToken,

    /// Handle to the task used to update the SEPTA information
    update_task_handle: Option<JoinHandle<Result<()>>>,
}

const SEPTA_IMAGE: &[u8] = include_bytes!("../assets/SEPTA_16.bmp");
lazy_static! {
    static ref SEPTA_BMP: Bmp::<'static, Rgb888> = Bmp::<Rgb888>::from_slice(SEPTA_IMAGE).unwrap();
}

impl UpcomingTrainsRender {
    pub(crate) fn new(station: RegionalRailStop) -> Self {
        let septa_api = septa_api::Client::new();

        let state = Arc::new(Mutex::new(UpcomingTrainsState::default()));
        let cancel_token = CancellationToken::new();

        let task_cancel_token = cancel_token.clone();
        let task_state = state.clone();
        let task_station = station.clone();

        let update_task_handle: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            loop {
                match septa_api
                    .arrivals(ArrivalsRequest {
                        station: task_station.clone(),
                        results: Some(3),
                        direction: None,
                    })
                    .await
                {
                    Ok(response) => {
                        let mut state_unlocked = task_state.lock();
                        state_unlocked.northbound = response.northbound;
                        state_unlocked.southbound = response.southbound;

                        println!(
                            "northbound: {:?}, southbound: {:?}",
                            state_unlocked.northbound, state_unlocked.southbound
                        );
                    }
                    Err(e) => error!("Could not get updated information {e}"),
                }

                select! {
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {},
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

impl<D: DrawTarget<Color = Rgb888, Error = Infallible>> Render<D> for UpcomingTrainsRender {
    fn render(&self, canvas: &mut D) -> Result<()> {
        let station_name = self.station.to_string();
        let state_unlocked = self.state.lock();
        let ntime;
        let stime;

        let northbound_train_layout = if let Some(train) = state_unlocked.northbound.get(0) {
            let text_color = match train.status.as_str() {
                "On Time" => Rgb888::GREEN,
                _ => Rgb888::RED,
            };
            ntime = train.sched_time.format("%_H:%M").to_string();

            let chain = Chain::new(Text::new(
                &train.train_id,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_6X9, Rgb888::WHITE),
            ))
            .append(Text::new(
                &ntime,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, text_color),
            ))
            .append(Text::new(
                train.status.as_str(),
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, text_color),
            ));

            LinearLayout::horizontal(chain)
                .with_alignment(vertical::Center)
                .with_spacing(spacing::FixedMargin(6))
                .arrange()
        } else {
            let chain = Chain::new(Text::new(
                "No Train",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_6X9, Rgb888::WHITE),
            ))
            .append(Text::new(
                "No Status",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ))
            .append(Text::new(
                "No Status",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ));

            LinearLayout::horizontal(chain)
                .with_alignment(vertical::Center)
                .with_spacing(spacing::FixedMargin(6))
                .arrange()
        };

        let southbound_train_layout = if let Some(train) = state_unlocked.southbound.get(0) {
            let text_color = match train.status.as_str() {
                "On Time" => Rgb888::GREEN,
                _ => Rgb888::RED,
            };
            stime = train.sched_time.format("%_H:%M").to_string();

            let chain = Chain::new(Text::new(
                &train.train_id,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_6X9, Rgb888::WHITE),
            ))
            .append(Text::new(
                &stime,
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, text_color),
            ))
            .append(Text::new(
                train.status.as_str(),
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, text_color),
            ));

            LinearLayout::horizontal(chain)
                .with_alignment(vertical::Center)
                .with_spacing(spacing::FixedMargin(6))
                .arrange()
        } else {
            let chain = Chain::new(Text::new(
                "No Train",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_6X9, Rgb888::WHITE),
            ))
            .append(Text::new(
                "No Status",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ))
            .append(Text::new(
                "No Status",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_5X7, Rgb888::WHITE),
            ));

            LinearLayout::horizontal(chain)
                .with_alignment(vertical::Center)
                .with_spacing(spacing::FixedMargin(6))
                .arrange()
        };

        LinearLayout::vertical(
            Chain::new(
                LinearLayout::horizontal(
                    Chain::new(Text::new(
                        &station_name,
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::ascii::FONT_9X15, Rgb888::WHITE),
                    ))
                    .append(Image::new(&*SEPTA_BMP, Point::zero())),
                )
                .with_spacing(spacing::FixedMargin(6))
                .arrange(),
            )
            .append(Text::new(
                "Northbound",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_7X13_BOLD, Rgb888::WHITE),
            ))
            .append(northbound_train_layout)
            .append(Text::new(
                "Southbound",
                Point::zero(),
                MonoTextStyle::new(&mono_font::ascii::FONT_7X13_BOLD, Rgb888::WHITE),
            ))
            .append(southbound_train_layout),
        )
        .arrange()
        .draw(canvas)
        .unwrap();

        Ok(())
    }
}

impl Drop for UpcomingTrainsRender {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.update_task_handle.take() {
            task_handle.abort();
        }
    }
}
