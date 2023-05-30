use anyhow::{anyhow, Result};
use geoutils::{Distance, Location};
use log::debug;
use septa_api::{responses::Train, types::RegionalRailStop};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;

/// The amount of time the user has to be within the radius of a station to be considered at the station.
const NO_STATUS_TO_AT_STATION: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct NoStatusTracker {
    /// A map of the regional rail stop to the time the user entered into the radius of the station.
    /// We use time::Instance since we need a monotonic clock and do not care about the system time.
    station_to_first_encounter: HashMap<RegionalRailStop, Instant>,
}

#[derive(Debug, Default)]
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
const AT_STATION_TO_NO_STATUS_TIMEOUT: Duration = Duration::from_secs(180);

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    static ref AT_STATION_LEAVE_RADIUS: Distance = Distance::from_meters(200.0);
}

#[derive(Debug)]
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

#[derive(Debug)]
struct TrainTracker {
    /// The unique train id.
    train_id: String,

    /// The time the user has been on the train.
    last_train_encounter: Instant,
}

#[derive(Debug)]
enum State {
    NoStatus(NoStatusTracker),
    AtStation(StationTracker),
    OnTrain(TrainTracker),
}

pub(crate) struct TransitState {
    state: State,
}

impl TransitState {
    fn new() -> Self {
        Self {
            state: State::NoStatus(NoStatusTracker {
                station_to_first_encounter: HashMap::new(),
            }),
        }
    }

    fn update_state(mut self, lat_lon: (f64, f64), trains: Vec<Train>) -> Result<Self> {
        // Get the monotonic time
        let now = Instant::now();
        let person_location = Location::new(lat_lon.0, lat_lon.1);

        self.state = match self.state {
            State::NoStatus(mut tracker) => {
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
                    0 => State::NoStatus(tracker),
                    1 => {
                        let station = eligible_stations[0].clone();

                        debug!(
                            "Transitioning from NoStatus to AtStation (station: {})",
                            station.to_string()
                        );
                        State::AtStation(StationTracker {
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
                        State::AtStation(StationTracker {
                            station: closest_station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        })
                    }
                }
            }
            State::AtStation(mut tracker) => {
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
                    State::NoStatus(NoStatusTracker::default())
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
                            State::OnTrain(TrainTracker {
                                train_id,
                                last_train_encounter: now,
                            })
                        }
                        None => State::AtStation(tracker),
                    }
                }
            }
            State::OnTrain(mut tracker) => {
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
                            State::OnTrain(tracker)
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
                                    State::AtStation(StationTracker {
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
                                    State::NoStatus(NoStatusTracker::default())
                                }
                            }
                        } else {
                            State::OnTrain(tracker)
                        }
                    }
                    None => State::NoStatus(NoStatusTracker {
                        station_to_first_encounter: HashMap::new(),
                    }),
                }
            }
        };

        Ok(self)
    }
}

struct TransitTracker {
    client: septa_api::Client,
    states: HashMap<String, TransitState>,
}

impl TransitTracker {
    fn new(people: Vec<String>) -> Self {
        let mut states = HashMap::new();

        for person in people {
            states.insert(person, TransitState::new());
        }

        Self {
            client: septa_api::Client::new(),
            states,
        }
    }
}
